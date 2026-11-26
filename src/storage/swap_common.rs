// What the application and the swap helper must agree on: where the staging
// directory is, how a failure is logged, how the file the helper may leave
// next to the exe is named, and the lock that keeps a swap and a session of
// the same exe apart.
//
// This file is compiled into both binaries: `storage` declares it as a
// module, and `src/bin/locknote_swap.rs` includes it with `#[path]`. Each
// binary uses only a part of it.

use sha2::{Digest, Sha256};
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Name of the log shared by the application and the helper, in the staging
/// directory. One line per event: `[unix seconds] message`.
pub const SWAP_LOG_NAME: &str = "swap-error.log";

/// Extension of the companion file of a staging file: `{hash}.base` next to
/// `{hash}.tmp` (see `storage::Companion`). The helper removes it with the
/// `.tmp` it has moved over the exe.
pub const COMPANION_EXTENSION: &str = "base";

/// Last argument of the helper when the user asked to restart LockNote after
/// an update: once the exe is replaced, the helper starts it again.
pub const RELAUNCH_ARG: &str = "--relaunch";

/// How long the application waits at startup for a swap of its exe to
/// finish, and how long a helper waits for the application to hand the swap
/// lock over.
pub const SWAP_LOCK_WAIT: Duration = Duration::from_secs(60);

/// How long the application, at exit, waits for the helper it has just
/// started to queue on the swap lock before it releases that lock.
pub const HELPER_READY_WAIT: Duration = Duration::from_secs(15);

/// The 16 hex digits that name everything LockNote keeps for one exe: the
/// first 8 bytes of SHA-256 of the exe path in upper case, lowercase hex.
/// Case-insensitive, as Windows paths are.
pub fn exe_hash(exe_path: &Path) -> String {
    let path_str = exe_path.to_string_lossy().to_uppercase();
    let hash = Sha256::digest(path_str.as_bytes());
    hash[..8].iter().map(|b| format!("{:02x}", b)).collect()
}

/// Name of the named mutex held while the staging file of `exe_path` is
/// moved over the exe (by the helper) and during a session of that exe (by
/// the application): `Local\LockNote_Swap_{hash}`.
pub fn swap_lock_name(exe_path: &Path) -> String {
    swap_lock_name_for_hash(&exe_hash(exe_path))
}

/// `swap_lock_name` from the hash already computed (the name of a `.tmp`).
pub fn swap_lock_name_for_hash(hash: &str) -> String {
    format!("Local\\LockNote_Swap_{}", hash)
}

/// A named mutex owned by the calling thread. Dropping it releases it; it
/// must be dropped on the thread that acquired it (Win32 rule).
#[cfg(windows)]
pub struct SwapLock {
    handle: isize,
    name: String,
}

/// How a `SwapLock` was obtained.
#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Obtained {
    /// Released normally by its previous owner, or never held.
    Free,
    /// Its previous owner ended without releasing it (crash, kill, or the
    /// application that held it during its session exited).
    Abandoned,
}

#[cfg(windows)]
impl SwapLock {
    /// Wait up to `timeout` for the mutex `name`.
    pub fn acquire(name: &str, timeout: Duration) -> Result<(SwapLock, Obtained), String> {
        Self::acquire_inner(name, None, timeout).map_err(LockError::into_message)
    }

    /// Take the mutex `name` without waiting: `Ok(None)` when someone else
    /// holds it.
    pub fn try_acquire(name: &str) -> Result<Option<(SwapLock, Obtained)>, String> {
        match Self::acquire_inner(name, None, Duration::ZERO) {
            Ok(got) => Ok(Some(got)),
            Err(LockError::Held(_)) => Ok(None),
            Err(LockError::Failed(e)) => Err(e),
        }
    }

    /// Signal the event `ready` and start waiting for the mutex `name` in one
    /// atomic step (`SignalObjectAndWait`): whoever waits for `ready` knows
    /// that this thread is already queued on the mutex.
    pub fn acquire_signalling(
        name: &str,
        ready: &str,
        timeout: Duration,
    ) -> Result<(SwapLock, Obtained), String> {
        Self::acquire_inner(name, Some(ready), timeout).map_err(LockError::into_message)
    }

    fn acquire_inner(
        name: &str,
        ready: Option<&str>,
        timeout: Duration,
    ) -> Result<(SwapLock, Obtained), LockError> {
        use windows::core::HSTRING;
        use windows::Win32::Foundation::{
            CloseHandle, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT,
        };
        use windows::Win32::System::Threading::{
            CreateMutexW, OpenEventW, SignalObjectAndWait, WaitForSingleObject,
            EVENT_MODIFY_STATE,
        };
        let millis = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX - 1);
        unsafe {
            let mutex = CreateMutexW(None, false, &HSTRING::from(name))
                .map_err(|e| LockError::Failed(format!("create mutex {}: {}", name, e)))?;
            let waited = match ready {
                None => WaitForSingleObject(mutex, millis),
                Some(ready_name) => {
                    let event = match OpenEventW(EVENT_MODIFY_STATE, false, &HSTRING::from(ready_name)) {
                        Ok(h) => h,
                        Err(e) => {
                            let _ = CloseHandle(mutex);
                            return Err(LockError::Failed(format!("open event {}: {}", ready_name, e)));
                        }
                    };
                    let w = SignalObjectAndWait(event, mutex, millis, false);
                    let _ = CloseHandle(event);
                    w
                }
            };
            let obtained = if waited == WAIT_OBJECT_0 {
                Obtained::Free
            } else if waited == WAIT_ABANDONED {
                Obtained::Abandoned
            } else {
                let error = if waited == WAIT_TIMEOUT {
                    LockError::Held(format!("mutex {}: not released within {} s", name, timeout.as_secs()))
                } else {
                    LockError::Failed(format!("mutex {}: wait failed: {}", name, windows::core::Error::from_thread()))
                };
                let _ = CloseHandle(mutex);
                return Err(error);
            };
            Ok((SwapLock { handle: mutex.0 as isize, name: name.to_string() }, obtained))
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Why a `SwapLock` was not obtained.
#[cfg(windows)]
enum LockError {
    /// Someone else still held it when the wait ended.
    Held(String),
    /// The mutex or the ready event could not be used.
    Failed(String),
}

#[cfg(windows)]
impl LockError {
    fn into_message(self) -> String {
        match self {
            LockError::Held(e) | LockError::Failed(e) => e,
        }
    }
}

#[cfg(windows)]
impl Drop for SwapLock {
    fn drop(&mut self) {
        use windows::Win32::Foundation::{CloseHandle, HANDLE};
        use windows::Win32::System::Threading::ReleaseMutex;
        let h = HANDLE(self.handle as *mut core::ffi::c_void);
        unsafe {
            if let Err(e) = ReleaseMutex(h) {
                // Only possible when dropped on another thread than the
                // owner: a bug. The process exit abandons the mutex anyway.
                eprintln!("swap lock {}: release failed: {}", self.name, e);
            }
            let _ = CloseHandle(h);
        }
    }
}

/// Extension of the copy the helper writes next to the exe when the staging
/// file is on another volume. Neither `.exe` nor `.new` (`.new` is the name
/// `scripts/migrate-exe.ps1` uses and checks).
pub const SIBLING_EXTENSION: &str = "lnswap";

/// Length of the random part of a sibling or part-file name (hex digits).
pub const RANDOM_ID_LEN: usize = 16;

/// The swap log of the staging directory.
pub fn swap_log_path() -> PathBuf {
    staging_dir().join(SWAP_LOG_NAME)
}

/// The staging directory: `%LOCALAPPDATA%\LockNote`.
pub fn staging_dir() -> PathBuf {
    // Prefer %LOCALAPPDATA%\LockNote. Fall back to the OS temp dir if the
    // env var is missing (happens on unusual SYSTEM-account invocations
    // or child processes with a scrubbed environment). Panicking here
    // would just crash the app at startup — silently degrading to
    // temp_dir keeps LockNote usable.
    let base = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());

    base.join("LockNote")
}

/// Append one dated line to the swap log of `dir`.
pub fn append_log_in(dir: &Path, msg: &str) -> io::Result<()> {
    use std::io::Write;
    std::fs::create_dir_all(dir)?;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let line = format!("[{}] {}\n", ts, msg);
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(SWAP_LOG_NAME))?;
    f.write_all(line.as_bytes())
}

/// Append one dated line to the swap log of the staging directory.
///
/// The log is where failures are reported: when the log itself cannot be
/// written there is nowhere left to report to but stderr (which a GUI
/// process usually does not have).
pub fn append_log(msg: &str) {
    if let Err(e) = append_log_in(&staging_dir(), msg) {
        eprintln!("swap log: cannot write ({}): {}", e, msg);
    }
}

/// `RANDOM_ID_LEN` random lowercase hex digits.
pub fn random_id() -> String {
    use rand::Rng;
    let mut bytes = [0u8; RANDOM_ID_LEN / 2];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn is_random_id(s: &str) -> bool {
    s.len() == RANDOM_ID_LEN && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Name of the copy written next to `exe_name`: `<exe name>.<id>.lnswap`.
pub fn sibling_name(exe_name: &str, id: &str) -> String {
    format!("{}.{}.{}", exe_name, id, SIBLING_EXTENSION)
}

/// Whether `name` is exactly a name `sibling_name` gives for `exe_name`
/// (the exe name is compared without case, as Windows does).
pub fn is_sibling_name(name: &str, exe_name: &str) -> bool {
    is_suffixed_name(name, exe_name, SIBLING_EXTENSION)
}

/// Whether `name` is `<base>.<random id>.<extension>`.
pub fn is_suffixed_name(name: &str, base: &str, extension: &str) -> bool {
    // "." + id + "." + extension
    let tail_len = 1 + RANDOM_ID_LEN + 1 + extension.len();
    if name.len() != base.len() + tail_len || !name.is_char_boundary(base.len()) {
        return false;
    }
    let (head, tail) = name.split_at(base.len());
    if !head.eq_ignore_ascii_case(base) {
        return false;
    }
    let Some(rest) = tail.strip_prefix('.') else {
        return false;
    };
    let Some((id, ext)) = rest.split_once('.') else {
        return false;
    };
    is_random_id(id) && ext == extension
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sibling_names_are_recognized_exactly() {
        let name = sibling_name("LockNote-Pro.exe", "0123456789abcdef");
        assert_eq!(name, "LockNote-Pro.exe.0123456789abcdef.lnswap");
        assert!(is_sibling_name(&name, "LockNote-Pro.exe"));
        assert!(is_sibling_name(&name, "locknote-pro.EXE"), "Windows names ignore case");
        assert!(is_sibling_name(&sibling_name("a.exe", &random_id()), "a.exe"));

        for other in [
            "LockNote-Pro.exe",
            "LockNote-Pro.exe.new",
            "LockNote-Pro.exe.lnswap",
            "LockNote-Pro.exe.0123456789abcde.lnswap",   // 15 digits
            "LockNote-Pro.exe.0123456789abcdef0.lnswap", // 17 digits
            "LockNote-Pro.exe.0123456789ABCDEF.lnswap",  // not what random_id gives
            "LockNote-Pro.exe.0123456789abcdeg.lnswap",
            "LockNote-Pro.exe.0123456789abcdef.lnswap.bak",
            "LockNote-Pro.exe.0123456789abcdef.exe",
            "LockNote-Pro.exe-0123456789abcdef.lnswap",
            "Other.exe.0123456789abcdef.lnswap",
            "xLockNote-Pro.exe.0123456789abcdef.lnswap",
            "",
        ] {
            assert!(!is_sibling_name(other, "LockNote-Pro.exe"), "{:?}", other);
        }
        // Multi-byte names must not panic on the split.
        assert!(!is_sibling_name("é.exe.0123456789abcdef.lnswapé", "ée.exe"));
        assert!(is_sibling_name("é.exe.0123456789abcdef.lnswap", "é.exe"));
    }

    #[test]
    fn exe_hash_keeps_the_name_of_existing_staging_files() {
        // First 8 bytes of SHA-256("C:\TEST\APP.EXE"), the name every version
        // has given the .tmp of that exe: a pending .tmp left by 1.2.23 must
        // be found under the same name.
        assert_eq!(exe_hash(Path::new("c:\\test\\app.exe")), "75f25966e9fee863");
        assert_eq!(exe_hash(Path::new("C:\\TEST\\APP.EXE")), "75f25966e9fee863");
        assert_eq!(
            swap_lock_name(Path::new("C:\\Test\\App.exe")),
            "Local\\LockNote_Swap_75f25966e9fee863"
        );
    }

    #[cfg(windows)]
    #[test]
    fn swap_lock_is_exclusive_between_threads_and_released_on_drop() {
        let name = format!("Local\\LockNote_Swap_test_{}", random_id());
        let (lock, obtained) = SwapLock::acquire(&name, Duration::from_secs(1)).unwrap();
        assert_eq!(obtained, Obtained::Free);
        let other = name.clone();
        let blocked = std::thread::spawn(move || SwapLock::acquire(&other, Duration::from_millis(200)).map(|_| ()))
            .join()
            .unwrap();
        let err = blocked.expect_err("held by another thread");
        assert!(err.contains("not released within"), "{}", err);
        drop(lock);
        let other = name.clone();
        let free = std::thread::spawn(move || SwapLock::acquire(&other, Duration::from_millis(200)).map(|(_, o)| o))
            .join()
            .unwrap();
        assert_eq!(free, Ok(Obtained::Free));
    }

    #[cfg(windows)]
    #[test]
    fn swap_lock_left_by_a_dead_owner_is_reported_abandoned() {
        let name = format!("Local\\LockNote_Swap_test_{}", random_id());
        let owner = name.clone();
        std::thread::spawn(move || {
            let (lock, _) = SwapLock::acquire(&owner, Duration::from_secs(1)).unwrap();
            std::mem::forget(lock); // the owner ends without releasing
        })
        .join()
        .unwrap();
        let (_lock, obtained) = SwapLock::acquire(&name, Duration::from_secs(1)).unwrap();
        assert_eq!(obtained, Obtained::Abandoned);
    }

    #[cfg(windows)]
    #[test]
    fn signalling_without_the_event_fails_without_waiting() {
        let name = format!("Local\\LockNote_Swap_test_{}", random_id());
        let err = SwapLock::acquire_signalling(&name, "Local\\LockNote_SwapReady_missing_x", Duration::from_secs(5))
            .map(|_| ())
            .expect_err("no such event");
        assert!(err.contains("open event"), "{}", err);
    }

    #[test]
    fn random_ids_are_hex_and_differ() {
        let a = random_id();
        let b = random_id();
        assert!(is_random_id(&a), "{}", a);
        assert!(is_random_id(&b), "{}", b);
        assert_ne!(a, b);
    }

    #[test]
    fn log_lines_are_dated_and_appended() {
        let dir = std::env::temp_dir().join(format!(
            "locknote_swaplog_{}_{}",
            std::process::id(),
            random_id()
        ));
        append_log_in(&dir, "first").unwrap();
        append_log_in(&dir, "second").unwrap();
        let log = std::fs::read_to_string(dir.join(SWAP_LOG_NAME)).unwrap();
        let lines: Vec<&str> = log.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with('[') && lines[0].ends_with("] first"), "{}", lines[0]);
        assert!(lines[1].ends_with("] second"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
