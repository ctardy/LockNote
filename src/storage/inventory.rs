// What the staging directory holds, for the "Temporary files" dialog (issue
// 057): every pending `.tmp`, which exe it belongs to, and whether that exe
// is gone while its folder is still there on the same volume (an orphan, the
// only `.tmp` the user may delete, by hand, after a confirmation).
//
// Nothing here runs on its own: listing reads, and `delete_orphan_in` deletes
// one `.tmp` only when the user asked for it and the file is still an orphan
// under the swap lock of its exe.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::decision::Companion;
use super::swap_common;

/// Who a pending `.tmp` belongs to, as far as can be told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Owner {
    /// The `.tmp` of the exe this process runs from.
    Current,
    /// Its companion names an exe that exists.
    ExePresent(PathBuf),
    /// Its companion names an exe that does not exist, in a folder that
    /// does, on the volume the companion recorded: the only case that may be
    /// deleted.
    Orphan(PathBuf),
    /// Neither the exe nor its folder exist (drive absent, share
    /// disconnected, folder removed).
    FolderMissing(PathBuf),
    /// The exe does not exist and its folder does, but on another volume
    /// than the one the companion recorded: the drive letter or share now
    /// names another disk (a USB key replaced by another one).
    OtherVolume(PathBuf),
    /// Whether the exe or its folder exists, or which volume the folder is
    /// on, cannot be told (an error other than "not found", or a companion
    /// that recorded no volume).
    Unreachable(PathBuf, String),
    /// No companion: left by a version <= 1.2.23, the exe is unknown.
    NoCompanion,
    /// The companion cannot be read, is malformed, or names an exe that
    /// cannot be trusted (another hash than the name of the `.tmp`, a
    /// relative path, a name that could not be written as it is).
    CompanionUnusable(String),
    /// The `.tmp` itself could not be examined (an error other than "not
    /// found": being deleted, access denied).
    TmpUnreadable(String),
}

impl Owner {
    /// Whether the user may delete this `.tmp`.
    pub fn deletable(&self) -> bool {
        matches!(self, Owner::Orphan(_))
    }

    /// The exe the companion names, when it was read.
    pub fn exe(&self) -> Option<&Path> {
        match self {
            Owner::ExePresent(p)
            | Owner::Orphan(p)
            | Owner::FolderMissing(p)
            | Owner::OtherVolume(p)
            | Owner::Unreachable(p, _) => Some(p),
            _ => None,
        }
    }
}

/// One pending `.tmp` of the staging directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedEntry {
    /// The 16 hex digits of its name (`swap_common::exe_hash` of its exe).
    pub hash: String,
    pub tmp: PathBuf,
    /// None when the file could not be examined (`Owner::TmpUnreadable`).
    pub size: Option<u64>,
    pub modified: Option<SystemTime>,
    pub owner: Owner,
}

/// The pending `.tmp` files of `dir`, sorted by name. A missing directory
/// is an empty list; an error reading the directory is returned; an error on
/// one file only concerns that file (`Owner::TmpUnreadable`), and a file
/// gone meanwhile is not listed.
pub fn list_staging_in(dir: &Path, current_exe: &Path) -> io::Result<Vec<StagedEntry>> {
    let read = match fs::read_dir(dir) {
        Ok(r) => r,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let current = swap_common::exe_hash(current_exe);
    let mut entries = Vec::new();
    for item in read {
        let item = item?;
        let name = item.file_name();
        let Some(hash) = name.to_str().and_then(staged_hash) else {
            continue;
        };
        let (size, modified, owner) = match fs::symlink_metadata(item.path()) {
            Ok(meta) if !meta.file_type().is_file() => continue,
            Ok(meta) => match meta.modified() {
                Ok(m) => (Some(meta.len()), Some(m), classify(dir, hash, &current)),
                Err(e) => (Some(meta.len()), None, Owner::TmpUnreadable(e.to_string())),
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => (None, None, Owner::TmpUnreadable(e.to_string())),
        };
        entries.push(StagedEntry { hash: hash.to_string(), tmp: item.path(), size, modified, owner });
    }
    entries.sort_by(|a, b| a.hash.cmp(&b.hash));
    Ok(entries)
}

/// The hash in `name` when it is exactly `{16 lowercase hex}.tmp`.
fn staged_hash(name: &str) -> Option<&str> {
    let hash = name.strip_suffix(".tmp")?;
    let hex = hash.len() == 16 && hash.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    hex.then_some(hash)
}

/// Who the `.tmp` named `hash` in `dir` belongs to (`current` = hash of the
/// exe this process runs from).
fn classify(dir: &Path, hash: &str, current: &str) -> Owner {
    if hash == current {
        return Owner::Current;
    }
    let companion = dir.join(format!("{}.{}", hash, swap_common::COMPANION_EXTENSION));
    let (exe, volume) = match read_companion_exe(&companion, hash) {
        Ok(Some(found)) => found,
        Ok(None) => return Owner::NoCompanion,
        Err(why) => return Owner::CompanionUnusable(why),
    };
    match fs::symlink_metadata(&exe) {
        Ok(_) => Owner::ExePresent(exe),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            let Some(parent) = exe.parent() else {
                return Owner::Unreachable(exe, "no folder in the path".to_string());
            };
            match fs::metadata(parent) {
                // A recorded 0 tells no volume apart (see `volume_serial`).
                Ok(m) if m.is_dir() => match (volume.filter(|v| *v != 0), volume_serial(parent)) {
                    (None, _) => Owner::Unreachable(exe, "the companion does not record its volume".to_string()),
                    (Some(recorded), Ok(now)) if recorded == now => Owner::Orphan(exe),
                    (Some(_), Ok(_)) => Owner::OtherVolume(exe),
                    (Some(_), Err(e)) => Owner::Unreachable(exe, format!("volume of its folder: {}", e)),
                },
                Ok(_) => Owner::Unreachable(exe, "its folder is not a folder".to_string()),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Owner::FolderMissing(exe),
                Err(e) => Owner::Unreachable(exe, e.to_string()),
            }
        }
        Err(e) => Owner::Unreachable(exe, e.to_string()),
    }
}

/// The exe named by the companion at `path` and the volume it recorded:
/// None when there is no companion, an error when it cannot be read, is
/// malformed, or names an exe that cannot be trusted.
fn read_companion_exe(path: &Path, hash: &str) -> Result<Option<(PathBuf, Option<u32>)>, String> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_file() => {}
        Ok(_) => return Err("the companion is not a regular file".to_string()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("the companion cannot be read: {}", e)),
    }
    let bytes = fs::read(path).map_err(|e| format!("the companion cannot be read: {}", e))?;
    let text = String::from_utf8(bytes).map_err(|_| "the companion is not UTF-8 text".to_string())?;
    let (c, volume) =
        Companion::parse_with_volume(&text).map_err(|e| format!("the companion is malformed: {}", e))?;
    if swap_common::exe_hash(&c.exe) != hash {
        return Err(format!("the companion names another program file ({})", c.exe.display()));
    }
    // A name Windows allows but UTF-8 cannot hold was written with U+FFFD in
    // its place: that path names no file, whether the exe exists or not.
    if c.exe.to_string_lossy().contains('\u{FFFD}') {
        return Err(format!("the companion holds an altered file name ({})", c.exe.display()));
    }
    // "C:name.exe" or "name.exe" would be resolved against the current folder.
    if !c.exe.is_absolute() {
        return Err(format!("the companion holds a relative path ({})", c.exe.display()));
    }
    Ok(Some((c.exe, volume)))
}

/// Serial number of the volume that holds `path` (a file or a folder).
#[cfg(windows)]
pub(super) fn volume_serial(path: &Path) -> io::Result<u32> {
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Storage::FileSystem::{GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION};
    const FILE_READ_ATTRIBUTES: u32 = 0x0080;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000; // needed to open a folder
    const FILE_SHARE_ALL: u32 = 0x1 | 0x2 | 0x4; // read, write, delete
    let file = fs::OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .share_mode(FILE_SHARE_ALL)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?;
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    unsafe { GetFileInformationByHandle(windows::Win32::Foundation::HANDLE(file.as_raw_handle()), &mut info) }
        .map_err(io::Error::other)?;
    // Some virtual file systems (RAM disks, user-mode mounts) answer 0 for
    // every volume: that number tells nothing apart.
    if info.dwVolumeSerialNumber == 0 {
        return Err(io::Error::other("the volume has no serial number (0)"));
    }
    Ok(info.dwVolumeSerialNumber)
}

#[cfg(not(windows))]
pub(super) fn volume_serial(_path: &Path) -> io::Result<u32> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "unsupported platform"))
}

/// Delete the orphan `.tmp` named `hash` and then its companion, once the
/// user has confirmed. Refused (nothing touched) for the exe this process
/// runs from, when the swap lock of that exe is held by someone else, or when
/// the file is no longer an orphan. Every outcome is logged in the swap log
/// of `dir`.
pub fn delete_orphan_in(dir: &Path, current_exe: &Path, hash: &str) -> Result<PathBuf, String> {
    let tmp = dir.join(format!("{}.tmp", hash));
    let outcome = delete_orphan_inner(dir, current_exe, hash, &tmp);
    match &outcome {
        Ok(exe) => super::log_in(
            dir,
            &format!("orphan staging file deleted by the user: tmp={} exe={}", tmp.display(), exe.display()),
        ),
        Err(why) => super::log_in(dir, &format!("WARN orphan staging file not deleted: {} tmp={}", why, tmp.display())),
    }
    outcome.map(|_| tmp)
}

/// `delete_orphan_in` without the log line; returns the absent exe.
fn delete_orphan_inner(dir: &Path, current_exe: &Path, hash: &str, tmp: &Path) -> Result<PathBuf, String> {
    if staged_hash(&format!("{}.tmp", hash)).is_none() {
        return Err("not the name of a staging file".to_string());
    }
    // Refused by name, whatever thread runs this: the swap lock of the open
    // exe would refuse it from another thread, but a Win32 mutex can be taken
    // again by the thread that owns it, so the lock alone is not enough.
    let current = swap_common::exe_hash(current_exe);
    if hash == current {
        return Err("it belongs to the LockNote file that is open".to_string());
    }
    match fs::symlink_metadata(tmp) {
        Ok(m) if m.file_type().is_file() => {}
        Ok(_) => return Err("not a regular file".to_string()),
        Err(e) => return Err(format!("cannot be read: {}", e)),
    }
    let lock = take_lock_now(dir, hash)?;
    let exe = match classify(dir, hash, &current) {
        Owner::Orphan(exe) => exe,
        other => return Err(format!("no longer an orphan ({:?})", other)),
    };
    fs::remove_file(tmp).map_err(|e| format!("cannot be deleted: {}", e))?;
    let companion = tmp.with_extension(swap_common::COMPANION_EXTENSION);
    let removed = match fs::remove_file(&companion) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    };
    drop(lock);
    removed.map_err(|e| {
        format!("the .tmp was deleted, but its companion {} could not be: {}", companion.display(), e)
    })?;
    Ok(exe)
}

/// The swap lock of the exe whose files are named `hash`, without waiting:
/// held by someone else means a session or a helper of that exe is at work.
#[cfg(windows)]
fn take_lock_now(dir: &Path, hash: &str) -> Result<swap_common::SwapLock, String> {
    let name = swap_common::swap_lock_name_for_hash(hash);
    match swap_common::SwapLock::try_acquire(&name)? {
        Some((lock, swap_common::Obtained::Free)) => Ok(lock),
        Some((lock, swap_common::Obtained::Abandoned)) => {
            super::log_in(dir, &format!("swap lock {} was abandoned by its previous owner; deletion goes on", name));
            Ok(lock)
        }
        None => Err("in use by a LockNote of that file (session or swap)".to_string()),
    }
}

#[cfg(not(windows))]
fn take_lock_now(_dir: &Path, _hash: &str) -> Result<(), String> {
    Err("unsupported platform".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// A directory unique to this test and process, holding `staging\` and
    /// `exes\` (where the exes the companions name live).
    struct Fixture {
        root: PathBuf,
        staging: PathBuf,
        exes: PathBuf,
    }

    impl Fixture {
        fn new(name: &str) -> Fixture {
            let root = std::env::temp_dir().join(format!(
                "locknote_inventory_{}_{}_{}",
                name,
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&root);
            let staging = root.join("staging");
            let exes = root.join("exes");
            fs::create_dir_all(&staging).unwrap();
            fs::create_dir_all(&exes).unwrap();
            Fixture { root, staging, exes }
        }

        fn current(&self) -> PathBuf {
            self.exes.join("current.exe")
        }

        fn tmp_of(&self, exe: &Path) -> PathBuf {
            self.staging.join(format!("{}.tmp", swap_common::exe_hash(exe)))
        }

        fn companion_of(&self, exe: &Path) -> PathBuf {
            self.tmp_of(exe).with_extension(swap_common::COMPANION_EXTENSION)
        }

        /// The volume of the test directory, as a companion records it.
        fn volume(&self) -> u32 {
            volume_serial(&self.exes).unwrap()
        }

        /// A `.tmp` for `exe`, with a companion naming it and recording
        /// `volume`.
        fn stage_on(&self, exe: &Path, volume: Option<u32>) {
            fs::write(self.tmp_of(exe), b"pending image").unwrap();
            let c = Companion { exe: exe.to_path_buf(), base: None, program_from_updater: false };
            fs::write(self.companion_of(exe), c.to_text_with_volume(volume)).unwrap();
        }

        /// `stage_on` the volume of the test directory.
        fn stage(&self, exe: &Path) {
            self.stage_on(exe, Some(self.volume()));
        }

        fn owner_of(&self, exe: &Path) -> Owner {
            let hash = swap_common::exe_hash(exe);
            let list = list_staging_in(&self.staging, &self.current()).unwrap();
            list.into_iter().find(|e| e.hash == hash).expect("listed").owner
        }

        fn log(&self) -> String {
            fs::read_to_string(self.staging.join(swap_common::SWAP_LOG_NAME)).unwrap_or_default()
        }

        /// Delete must refuse and leave both files as they were.
        fn assert_not_deleted(&self, exe: &Path) {
            let tmp_before = fs::read(self.tmp_of(exe)).unwrap();
            let companion_before = fs::read(self.companion_of(exe)).ok();
            let refused = delete_orphan_in(&self.staging, &self.current(), &swap_common::exe_hash(exe));
            assert!(refused.is_err(), "{}: {:?}", exe.display(), refused);
            assert_eq!(fs::read(self.tmp_of(exe)).unwrap(), tmp_before, "{}", exe.display());
            assert_eq!(fs::read(self.companion_of(exe)).ok(), companion_before, "{}", exe.display());
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn missing_directory_is_an_empty_list() {
        let f = Fixture::new("missing");
        let list = list_staging_in(&f.root.join("nowhere"), &f.current()).unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn each_owner_is_told_apart() {
        let f = Fixture::new("owners");
        let current = f.current();
        fs::write(&current, b"exe").unwrap();
        f.stage(&current);

        let present = f.exes.join("present.exe");
        fs::write(&present, b"exe").unwrap();
        f.stage(&present);

        let orphan = f.exes.join("gone.exe");
        f.stage(&orphan);

        let folder_missing = f.root.join("removed-folder").join("app.exe");
        f.stage(&folder_missing);

        let legacy = f.exes.join("legacy.exe");
        fs::write(f.tmp_of(&legacy), b"pending image").unwrap();

        let malformed = f.exes.join("malformed.exe");
        fs::write(f.tmp_of(&malformed), b"pending image").unwrap();
        fs::write(f.companion_of(&malformed), "not a companion").unwrap();

        // A companion that names another exe than the one its name is for.
        let wrong = f.exes.join("wrong.exe");
        fs::write(f.tmp_of(&wrong), b"pending image").unwrap();
        let other = Companion { exe: f.exes.join("other.exe"), base: None, program_from_updater: false };
        fs::write(f.companion_of(&wrong), other.to_text()).unwrap();

        assert_eq!(f.owner_of(&current), Owner::Current);
        assert_eq!(f.owner_of(&present), Owner::ExePresent(present.clone()));
        assert_eq!(f.owner_of(&orphan), Owner::Orphan(orphan.clone()));
        assert_eq!(f.owner_of(&folder_missing), Owner::FolderMissing(folder_missing.clone()));
        assert_eq!(f.owner_of(&legacy), Owner::NoCompanion);
        assert!(matches!(f.owner_of(&malformed), Owner::CompanionUnusable(_)), "{:?}", f.owner_of(&malformed));
        match f.owner_of(&wrong) {
            Owner::CompanionUnusable(why) => assert!(why.contains("other.exe"), "{}", why),
            o => panic!("{:?}", o),
        }

        let list = list_staging_in(&f.staging, &current).unwrap();
        assert_eq!(list.len(), 7);
        let deletable: Vec<_> = list.iter().filter(|e| e.owner.deletable()).map(|e| e.tmp.clone()).collect();
        assert_eq!(deletable, vec![f.tmp_of(&orphan)], "only the orphan may be deleted");
        let mut names: Vec<_> = list.iter().map(|e| e.hash.clone()).collect();
        let sorted = { let mut s = names.clone(); s.sort(); s };
        assert_eq!(names, sorted, "sorted by name");
        names.dedup();
        assert_eq!(names.len(), 7);
    }

    // Refuter A1: a drive letter that now names another volume (a USB key
    // replaced by another) is not an orphan, even at the root of the drive.
    #[test]
    fn exe_missing_on_another_volume_is_not_an_orphan() {
        let f = Fixture::new("other_volume");
        let replaced = f.exes.join("on-the-old-key.exe");
        f.stage_on(&replaced, Some(f.volume().wrapping_add(1)));
        assert_eq!(f.owner_of(&replaced), Owner::OtherVolume(replaced.clone()));
        f.assert_not_deleted(&replaced);
    }

    #[test]
    fn companion_without_volume_is_never_an_orphan() {
        // No volume line, or the 0 some virtual file systems give every
        // volume (refuter, pass 2, C2).
        for (name, volume) in [("none", None), ("zero", Some(0))] {
            let f = Fixture::new(&format!("no_volume_{}", name));
            let exe = f.exes.join("gone.exe");
            f.stage_on(&exe, volume);
            match f.owner_of(&exe) {
                Owner::Unreachable(p, why) => {
                    assert_eq!(p, exe);
                    assert!(why.contains("volume"), "{}", why);
                }
                o => panic!("{:?}: {:?}", volume, o),
            }
            f.assert_not_deleted(&exe);
        }
    }

    // Reviewer R1: the folder of the exe exists but is not a folder, or the
    // path cannot even be examined.
    #[test]
    fn unreachable_exe_is_not_an_orphan() {
        let f = Fixture::new("unreachable");
        fs::write(f.exes.join("file.txt"), b"x").unwrap();
        let under_a_file = f.exes.join("file.txt").join("app.exe");
        f.stage(&under_a_file);
        let bad_name = f.exes.join("a<b").join("x.exe");
        f.stage(&bad_name);
        for exe in [&under_a_file, &bad_name] {
            let owner = f.owner_of(exe);
            assert!(matches!(&owner, Owner::Unreachable(p, _) if p == exe), "{}: {:?}", exe.display(), owner);
            assert!(!owner.deletable());
            f.assert_not_deleted(exe);
        }
    }

    // Refuter A2 and A5: paths that name no file the companion meant.
    #[test]
    fn altered_or_relative_exe_paths_are_unusable() {
        let f = Fixture::new("untrusted");
        for exe in [
            f.exes.join("Lock\u{FFFD}.exe"),
            PathBuf::from("C:gone-relative.exe"),
            PathBuf::from("gone-relative.exe"),
        ] {
            f.stage(&exe);
            assert!(matches!(f.owner_of(&exe), Owner::CompanionUnusable(_)), "{}: {:?}", exe.display(), f.owner_of(&exe));
            f.assert_not_deleted(&exe);
        }
    }

    // Refuter A2, the real case: a file name with an unpaired surrogate. The
    // exe exists; what the companion can hold of its name does not.
    #[cfg(windows)]
    #[test]
    fn exe_named_with_an_unpaired_surrogate_is_never_deleted() {
        use std::os::windows::ffi::OsStringExt;
        let f = Fixture::new("surrogate");
        let mut wide: Vec<u16> = f.exes.join("Lock").as_os_str().to_string_lossy().encode_utf16().collect();
        wide.push(0xD800);
        wide.extend(".exe".encode_utf16());
        let exe = PathBuf::from(std::ffi::OsString::from_wide(&wide));
        fs::write(&exe, b"exe").unwrap();
        f.stage(&exe);
        let owner = f.owner_of(&exe);
        assert!(!owner.deletable(), "{:?}", owner);
        f.assert_not_deleted(&exe);
        assert!(exe.exists());
    }

    #[test]
    fn entry_reports_size_date_and_path() {
        let f = Fixture::new("entry");
        let orphan = f.exes.join("gone.exe");
        f.stage(&orphan);
        let list = list_staging_in(&f.staging, &f.current()).unwrap();
        let e = &list[0];
        assert_eq!(e.tmp, f.tmp_of(&orphan));
        assert_eq!(e.hash, swap_common::exe_hash(&orphan));
        assert_eq!(e.size, Some(b"pending image".len() as u64));
        assert_eq!(e.modified, Some(fs::metadata(f.tmp_of(&orphan)).unwrap().modified().unwrap()));
        assert_eq!(e.owner.exe(), Some(orphan.as_path()));
    }

    #[test]
    fn current_exe_is_current_even_without_companion_or_exe() {
        let f = Fixture::new("current_bare");
        fs::write(f.tmp_of(&f.current()), b"pending image").unwrap();
        assert_eq!(f.owner_of(&f.current()), Owner::Current);
    }

    #[test]
    fn current_exe_is_recognized_without_case() {
        let f = Fixture::new("current_case");
        let current = f.current();
        f.stage(&current);
        let upper = PathBuf::from(current.to_string_lossy().to_uppercase());
        let list = list_staging_in(&f.staging, &upper).unwrap();
        assert_eq!(list[0].owner, Owner::Current);
    }

    #[test]
    fn other_files_are_not_listed() {
        let f = Fixture::new("others");
        let exe = f.exes.join("gone.exe");
        let hash = swap_common::exe_hash(&exe);
        for name in [
            format!("{}.tmp.0123456789abcdef.part", hash),
            format!("{}.base", "0123456789abcdef"),
            "not-hex-at-all!.tmp".to_string(),
            "0123456789ABCDEF.tmp".to_string(),
            "0123456789abcde.tmp".to_string(),
            "swap-error.log".to_string(),
            "import.log".to_string(),
        ] {
            fs::write(f.staging.join(name), b"x").unwrap();
        }
        fs::create_dir_all(f.staging.join("fedcba9876543210.tmp")).unwrap();
        fs::create_dir_all(f.staging.join("conflicts")).unwrap();
        assert!(list_staging_in(&f.staging, &f.current()).unwrap().is_empty());
    }

    // Refuter A4 / Reviewer R6: files that come and go while the list is
    // read never make the whole list fail.
    #[test]
    fn files_deleted_while_listing_do_not_fail_the_list() {
        let f = Fixture::new("churn");
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let churn = {
            let (stop, staging) = (stop.clone(), f.staging.clone());
            std::thread::spawn(move || {
                let mut i = 0u64;
                while !stop.load(Ordering::Relaxed) {
                    let p = staging.join(format!("{:016x}.tmp", i % 8));
                    let _ = fs::write(&p, b"x");
                    let _ = fs::remove_file(&p);
                    i += 1;
                }
            })
        };
        for _ in 0..2000 {
            let listed = list_staging_in(&f.staging, &f.current());
            assert!(listed.is_ok(), "{:?}", listed.err());
        }
        stop.store(true, Ordering::Relaxed);
        churn.join().unwrap();
    }

    #[test]
    fn deleting_an_orphan_removes_tmp_then_companion_and_logs_it() {
        let f = Fixture::new("delete");
        let orphan = f.exes.join("gone.exe");
        f.stage(&orphan);
        let deleted = delete_orphan_in(&f.staging, &f.current(), &swap_common::exe_hash(&orphan)).unwrap();
        assert_eq!(deleted, f.tmp_of(&orphan));
        assert!(!f.tmp_of(&orphan).exists());
        assert!(!f.companion_of(&orphan).exists());
        let log = f.log();
        assert!(log.contains("orphan staging file deleted by the user"), "{}", log);
        assert!(log.contains(&f.tmp_of(&orphan).display().to_string()), "{}", log);
        assert!(log.contains(&orphan.display().to_string()), "{}", log);
    }

    #[test]
    fn deleting_is_refused_for_everything_but_an_orphan() {
        let f = Fixture::new("refused");
        let current = f.current();
        f.stage(&current); // current exe absent, folder present: still refused

        let present = f.exes.join("present.exe");
        fs::write(&present, b"exe").unwrap();
        f.stage(&present);

        let folder_missing = f.root.join("removed-folder").join("app.exe");
        f.stage(&folder_missing);

        let legacy = f.exes.join("legacy.exe");
        fs::write(f.tmp_of(&legacy), b"pending image").unwrap();

        let malformed = f.exes.join("malformed.exe");
        fs::write(f.tmp_of(&malformed), b"pending image").unwrap();
        fs::write(f.companion_of(&malformed), "not a companion").unwrap();

        for exe in [&current, &present, &folder_missing, &legacy, &malformed] {
            f.assert_not_deleted(exe);
        }
        let log = f.log();
        assert_eq!(log.matches("WARN orphan staging file not deleted").count(), 5, "{}", log);
        assert!(!log.contains("deleted by the user"), "{}", log);
    }

    #[test]
    fn deleting_an_unknown_hash_is_refused() {
        let f = Fixture::new("unknown");
        assert!(delete_orphan_in(&f.staging, &f.current(), "0123456789abcdef").is_err());
        assert!(delete_orphan_in(&f.staging, &f.current(), "..\\..\\x").is_err());
    }

    #[test]
    fn exe_back_between_listing_and_deleting_is_not_deleted() {
        let f = Fixture::new("back");
        let orphan = f.exes.join("gone.exe");
        f.stage(&orphan);
        assert!(f.owner_of(&orphan).deletable());
        fs::write(&orphan, b"exe").unwrap(); // the drive or the file came back
        f.assert_not_deleted(&orphan);
    }

    #[cfg(windows)]
    #[test]
    fn deleting_is_refused_while_another_process_holds_the_swap_lock() {
        let f = Fixture::new("locked");
        let orphan = f.exes.join("gone.exe");
        f.stage(&orphan);
        let name = swap_common::swap_lock_name(&orphan);
        let (held_tx, held_rx) = std::sync::mpsc::channel();
        let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
        let holder = std::thread::spawn(move || {
            let (_lock, _) = swap_common::SwapLock::acquire(&name, std::time::Duration::from_secs(1)).unwrap();
            held_tx.send(()).unwrap();
            done_rx.recv().unwrap();
        });
        held_rx.recv().unwrap();
        let refused = delete_orphan_in(&f.staging, &f.current(), &swap_common::exe_hash(&orphan));
        done_tx.send(()).unwrap();
        holder.join().unwrap();
        let why = refused.expect_err("lock held elsewhere");
        assert!(why.contains("in use"), "{}", why);
        assert!(f.tmp_of(&orphan).exists());
        assert!(f.companion_of(&orphan).exists());
        // Released: now it goes.
        delete_orphan_in(&f.staging, &f.current(), &swap_common::exe_hash(&orphan)).unwrap();
    }

    // Reviewer R2: a lock left by a dead owner is logged, then the deletion
    // goes on.
    #[cfg(windows)]
    #[test]
    fn abandoned_swap_lock_is_logged() {
        let f = Fixture::new("abandoned");
        let orphan = f.exes.join("gone.exe");
        f.stage(&orphan);
        let name = swap_common::swap_lock_name(&orphan);
        std::thread::spawn(move || {
            let (lock, _) = swap_common::SwapLock::acquire(&name, std::time::Duration::from_secs(1)).unwrap();
            std::mem::forget(lock); // the owner ends without releasing; its handle stays open
        })
        .join()
        .unwrap();
        delete_orphan_in(&f.staging, &f.current(), &swap_common::exe_hash(&orphan)).unwrap();
        let log = f.log();
        assert!(log.contains("was abandoned by its previous owner; deletion goes on"), "{}", log);
        assert!(log.contains("deleted by the user"), "{}", log);
    }
}
