// Storage module — Binary marker, read/write encrypted payload, .tmp management
//
// The LockNote binary format appends encrypted data after the PE executable:
//   [exe binary] [16-byte XOR-obfuscated marker] [encrypted payload]
//
// The running .exe cannot overwrite itself: every save writes the whole image
// to a .tmp staging file in %LOCALAPPDATA%\LockNote\, and a helper moves that
// file over the .exe once the process has exited (`promote_pending_on_exit`).
// Until that move succeeds, the .tmp is the only copy of the latest save: it
// is never deleted on a guess (see `PendingDecision`).
//
// Next to the .tmp, a companion file (`{hash}.base`, see `Companion`) records
// which notes of the exe the pending save was based on. Whether the .tmp is
// the latest state is decided by content (does the exe still hold that
// base?), never by modification dates, which a file server, a clock change or
// a copy can put in any order. Only a .tmp left by a version without
// companion is judged by dates, once, at startup.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::SystemTime;

// Shared with the swap helper binary, which uses the other part of it.
#[allow(dead_code)]
mod swap_common;

// The image format, the companion and the decision on a pending .tmp: shared
// with the swap helper binary, which re-checks the decision before it
// renames anything over the exe.
#[allow(dead_code)]
mod decision;

// The pending .tmp files of the staging directory and who they belong to,
// for the "Temporary files" dialog (issue 057).
pub mod inventory;

use decision::{exe_facts, exe_image, get_marker, notes_sha, same_exe, staged_facts, Sha, MARKER_LEN};
use decision::MIN_PAYLOAD_SIZE;
#[cfg(test)]
use decision::{sha, Image};
#[cfg(test)]
pub use decision::ConflictCause;
pub use decision::{
    decide_pending, find_marker, Companion, CompanionError, ExeFacts, IgnoreReason, PendingDecision,
    StagedFacts,
};

/// Magic bytes indicating a plaintext hint prefix is present.
/// Format after marker: [HINT_MAGIC (4 bytes)][u16 LE hint_len][hint UTF-8 bytes][encrypted payload]
const HINT_MAGIC: &[u8; 4] = b"HNT\x00";

/// Maximum hint length in bytes.
const MAX_HINT_LEN: usize = 100;

/// Extension of the file a staging write goes to before it takes the name of
/// the .tmp (or of its companion): `<name>.<random id>.part`.
const PART_EXTENSION: &str = "part";

/// Folder of the staging directory where the version set aside by a conflict
/// is kept (never deleted by LockNote).
const CONFLICTS_DIR: &str = "conflicts";

/// Files younger than this are left alone by the startup cleanup: they may
/// belong to a save or a swap in progress.
const CLEANUP_MIN_AGE: std::time::Duration = std::time::Duration::from_secs(60);

/// Returns a copy of the marker bytes (tests that build a LockNote file by
/// hand; the updater now stages through `write_staged_update`).
#[cfg(test)]
pub fn get_marker_for_update() -> Vec<u8> {
    get_marker().to_vec()
}

/// The staging directory: `%LOCALAPPDATA%\LockNote`.
pub fn staging_dir() -> PathBuf {
    swap_common::staging_dir()
}

/// The swap log of the staging directory (`swap-error.log`).
pub fn swap_log_path() -> PathBuf {
    swap_common::swap_log_path()
}

/// Write a line to the swap log of `dir` (`swap-error.log`, shared with the
/// helper). When the log itself cannot be written, stderr is all that is left.
fn log_in(dir: &Path, msg: &str) {
    if let Err(e) = swap_common::append_log_in(dir, msg) {
        eprintln!("swap log: cannot write ({}): {}", e, msg);
    }
}

/// Write a line to the swap log of the staging directory, for an event of
/// the staging file handled outside this module (the startup dialogs).
pub fn log_swap_event(msg: &str) {
    log_in(&staging_dir(), msg);
}

/// Compute the .tmp staging file path in %LOCALAPPDATA%\LockNote\.
///
/// The filename is the first 8 bytes of SHA256(exePath.ToUpperInvariant()) as
/// lowercase hex, ensuring case-insensitive uniqueness across exe locations.
pub fn get_tmp_path(exe_path: &Path) -> PathBuf {
    get_tmp_path_in(&staging_dir(), exe_path)
}

/// `get_tmp_path` with the staging directory given by the caller.
pub fn get_tmp_path_in(dir: &Path, exe_path: &Path) -> PathBuf {
    dir.join(format!("{}.tmp", swap_common::exe_hash(exe_path)))
}

/// The companion file of the .tmp of `exe_path`: `{hash}.base`.
pub fn companion_path_in(dir: &Path, exe_path: &Path) -> PathBuf {
    get_tmp_path_in(dir, exe_path).with_extension(swap_common::COMPANION_EXTENSION)
}

/// Extract the plaintext hint and encrypted data from a raw payload.
/// Returns (hint, encrypted_data). If no hint magic is found, hint is empty.
pub fn split_hint_and_encrypted(payload: &[u8]) -> (String, Vec<u8>) {
    if payload.len() >= 6 && &payload[..4] == HINT_MAGIC {
        let hint_len = u16::from_le_bytes([payload[4], payload[5]]) as usize;
        if hint_len <= MAX_HINT_LEN && 6 + hint_len <= payload.len() {
            let hint = String::from_utf8_lossy(&payload[6..6 + hint_len]).to_string();
            let encrypted = payload[6 + hint_len..].to_vec();
            return (hint, encrypted);
        }
    }
    // Old format or invalid hint header — entire payload is encrypted data
    (String::new(), payload.to_vec())
}

/// The companion of the .tmp of `exe_path`: None when there is no such file,
/// `Some(Err)` when it exists but cannot be read or used (`CompanionError`).
fn read_companion_in(dir: &Path, exe_path: &Path) -> Option<Result<Companion, CompanionError>> {
    decision::read_companion(&companion_path_in(dir, exe_path), exe_path)
}

/// Write the companion of the .tmp of `c.exe`, atomically, with the volume
/// serial number of the exe (issue 057: what tells an orphan from an exe
/// whose drive letter now names another volume). A volume that cannot be
/// read is logged and left out: that .tmp can then never be called an
/// orphan, and the save goes on.
fn write_companion_in(dir: &Path, c: &Companion) -> io::Result<()> {
    let path = companion_path_in(dir, &c.exe);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    // Logged once per process: a volume that cannot be read usually cannot
    // be read at any save of the session.
    static VOLUME_WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let volume = match inventory::volume_serial(&c.exe) {
        Ok(v) => Some(v),
        Err(e) => {
            if !VOLUME_WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                log_in(dir, &format!("WARN companion written without volume: {} exe={}", e, c.exe.display()));
            }
            None
        }
    };
    let text = c.to_text_with_volume(volume);
    write_staging_file(&path, |f| {
        use std::io::Write;
        f.write_all(text.as_bytes())
    })
}

#[cfg(test)]
thread_local! {
    /// Test hook of `pair_step`: (steps seen on this thread, the step that
    /// must fail). Set by `pending_tests` only.
    static PAIR_FAULT: std::cell::Cell<(u32, Option<u32>)> = const { std::cell::Cell::new((0, None)) };
}

/// Called before every step that writes, moves or removes the .tmp or its
/// companion. In tests it can make the n-th step of a thread fail
/// (`PAIR_FAULT`), to prove that a failure, or a crash, at any step leaves a
/// state the decision judges without loss or downgrade. Nothing in a
/// release build.
fn pair_step(what: &str) -> io::Result<()> {
    #[cfg(test)]
    {
        let (seen, fail_at) = PAIR_FAULT.with(|c| c.get());
        let seen = seen + 1;
        PAIR_FAULT.with(|c| c.set((seen, fail_at)));
        if fail_at == Some(seen) {
            return Err(io::Error::other(format!("injected failure at step {} ({})", seen, what)));
        }
    }
    #[cfg(not(test))]
    let _ = what;
    Ok(())
}

/// Remove the companion of `exe_path`. Ok(false): there was none.
fn remove_companion_in(dir: &Path, exe_path: &Path) -> io::Result<bool> {
    pair_step("remove companion")?;
    match fs::remove_file(companion_path_in(dir, exe_path)) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

// ── The order of the pair (.tmp, companion) ──
//
// Invariant: the companion on disk never claims more than the .tmp on disk
// justifies, whatever step fails or is cut short by a crash.
//
// - Writing (`stage_pair_in`, the only function that writes a .tmp): when a
//   .tmp is already there, its companion is first brought to what holds for
//   that .tmp AND for the new one (the base they share, else the current
//   one; `program=updater` only if both say it); then the .tmp is written;
//   then the final companion. A .tmp written again without a new save keeps
//   its modification date, so a .tmp without companion is still judged by
//   the same dates.
// - Removing: the .tmp goes first (moved over the exe, into `conflicts\`, or
//   deleted when stale); its companion is removed only once the .tmp is gone.
//
// A cut between two steps therefore leaves either the previous pair, or the
// new .tmp under a companion that claims less than the final one: the
// decision then opens, promotes or asks about the previous state, never
// promotes a .tmp on the strength of a claim made for another one. The proof
// is `pending_tests::every_failure_of_a_pair_step_leaves_a_state_judged_without_loss_or_downgrade`.

/// Write `image` as the .tmp of `target.exe`, with `target` as its final
/// companion, in the order of the pair (see above). `keep_modified`: the
/// modification date the new .tmp keeps (the .tmp is staged again without a
/// new save).
fn stage_pair_in(
    dir: &Path,
    target: &Companion,
    image: &[u8],
    keep_modified: Option<SystemTime>,
) -> io::Result<()> {
    let tmp_path = get_tmp_path_in(dir, &target.exe);
    if let Some(parent) = tmp_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp_there = match fs::symlink_metadata(&tmp_path) {
        Ok(_) => true,
        Err(e) if e.kind() == io::ErrorKind::NotFound => false,
        Err(e) => return Err(e),
    };
    let current = read_companion_in(dir, &target.exe);
    if let (true, Some(Err(CompanionError::Unreadable(e)))) = (tmp_there, &current) {
        // What the companion of the .tmp there claims is unknown: it cannot
        // be brought down first, so nothing is written (refuter pass 3, A1b).
        return Err(io::Error::other(e.clone()));
    }
    if !tmp_there {
        // Alone, a companion promotes nothing (removed at the next start).
        if current.as_ref() != Some(&Ok(target.clone())) {
            write_companion_in(dir, target)?;
        }
    } else if let Some(Ok(cur)) = &current {
        let meet = Companion {
            exe: target.exe.clone(),
            base: if cur.base == target.base { target.base } else { cur.base },
            program_from_updater: cur.program_from_updater && target.program_from_updater,
        };
        if &meet != cur {
            write_companion_in(dir, &meet)?;
        }
    }
    // A .tmp there without a usable companion is left judged as it is (by
    // dates, or as a conflict) until its final companion is written.
    write_staging_file(&tmp_path, |f| {
        use std::io::Write;
        f.write_all(image)?;
        if let Some(when) = keep_modified {
            f.set_modified(when)?;
        }
        Ok(())
    })?;
    if read_companion_in(dir, &target.exe).as_ref() != Some(&Ok(target.clone())) {
        write_companion_in(dir, target)?;
    }
    Ok(())
}

/// `[program part of program_source][marker][payload]`. `program_source` is
/// a LockNote file, with or without notes after its program.
fn build_image(program_source: &[u8], payload: &[u8]) -> Vec<u8> {
    let clean_len = find_marker(program_source).unwrap_or(program_source.len());
    let mut output = Vec::with_capacity(clean_len + MARKER_LEN + payload.len());
    output.extend_from_slice(&program_source[..clean_len]);
    output.extend_from_slice(&get_marker());
    output.extend_from_slice(payload);
    output
}

// ── What is done with an existing .tmp ──

/// The payload of a LockNote image: the bytes after the last marker.
fn payload_of(data: &[u8]) -> Result<&[u8], IgnoreReason> {
    let pos = find_marker(data).ok_or(IgnoreReason::NoMarker)?;
    let payload = &data[pos + MARKER_LEN..];
    if payload.len() < MIN_PAYLOAD_SIZE {
        return Err(IgnoreReason::PayloadTooShort(payload.len()));
    }
    Ok(payload)
}

/// Extract the encrypted payload from a file containing the marker.
fn read_payload_from_file(path: &Path) -> Option<Vec<u8>> {
    let data = fs::read(path).ok()?;
    payload_of(&data).ok().map(|p| p.to_vec())
}

/// Everything read to take the decision for one exe.
struct Inspection {
    decision: PendingDecision,
    tmp: Option<Result<StagedFacts, IgnoreReason>>,
    companion: Option<Result<Companion, CompanionError>>,
    /// Only read when the decision needed it.
    exe: Option<Result<ExeFacts, String>>,
}

fn inspect_in(dir: &Path, exe_path: &Path) -> Inspection {
    let tmp = staged_facts(&get_tmp_path_in(dir, exe_path));
    let companion = read_companion_in(dir, exe_path);
    let exe = std::cell::RefCell::new(None);
    let decision = decide_pending(tmp.as_ref(), companion.as_ref(), &|| {
        let facts = exe_facts(exe_path);
        *exe.borrow_mut() = Some(facts.clone());
        facts
    });
    Inspection { decision, tmp, companion, exe: exe.into_inner() }
}

/// What to do with the .tmp of `exe_path` in the staging directory `dir`.
pub fn pending_decision_in(dir: &Path, exe_path: &Path) -> PendingDecision {
    inspect_in(dir, exe_path).decision
}

/// Startup step, before the session is opened: apply what needs no
/// question, and return the decision (a `Conflict` is for the caller to put
/// to the user).
///
/// - a companion without .tmp is removed (the swap it belonged to is done);
/// - a stale .tmp is removed with its companion;
/// - an ignored .tmp, an unreadable exe and a conflict are logged;
/// - a .tmp that cannot be read, or whose companion file cannot be read (an
///   I/O error, or not a regular file), is `TmpUnreadable`: logged, nothing
///   changed; the caller shows the file named in it and quits. A companion
///   that is read but malformed is a conflict (`CompanionInvalid`);
/// - a .tmp to promote that has no companion (left by an earlier version)
///   receives one now, based on the notes the exe holds now. `Err` when that
///   companion cannot be written: nothing can then be opened safely.
pub fn settle_pending_at_startup(exe_path: &Path) -> Result<PendingDecision, String> {
    settle_pending_at_startup_in(&staging_dir(), exe_path)
}

/// `settle_pending_at_startup` with the staging directory given by the caller.
pub fn settle_pending_at_startup_in(dir: &Path, exe_path: &Path) -> Result<PendingDecision, String> {
    let ins = inspect_in(dir, exe_path);
    let tmp_path = get_tmp_path_in(dir, exe_path);
    let companion_path = companion_path_in(dir, exe_path);

    if ins.tmp.is_none() && ins.companion.is_some() {
        match pair_step("remove companion").and_then(|_| fs::remove_file(&companion_path)) {
            Ok(()) => log_in(
                dir,
                &format!("companion without .tmp removed: file={} exe={}", companion_path.display(), exe_path.display()),
            ),
            Err(e) => log_in(
                dir,
                &format!(
                    "companion without .tmp could not be removed: file={} exe={} err={}",
                    companion_path.display(),
                    exe_path.display(),
                    e
                ),
            ),
        }
    }

    match &ins.decision {
        PendingDecision::Ignored(reason) => log_in(
            dir,
            &format!(
                "pending .tmp ignored (neither promoted nor deleted): tmp={} exe={} reason={}",
                tmp_path.display(),
                exe_path.display(),
                reason
            ),
        ),
        PendingDecision::ExeUnreadable(e) => log_in(
            dir,
            &format!(
                "pending .tmp left alone, the program file cannot be read: tmp={} exe={} err={}",
                tmp_path.display(),
                exe_path.display(),
                e
            ),
        ),
        PendingDecision::TmpUnreadable(e) => log_in(
            dir,
            &format!(
                "pending .tmp left alone, it cannot be read (or its companion file cannot): tmp={} exe={} err={}",
                tmp_path.display(),
                exe_path.display(),
                e
            ),
        ),
        PendingDecision::Stale => {
            // Order of the pair: the companion goes only once its .tmp is gone.
            let removed = match pair_step("remove tmp").and_then(|_| fs::remove_file(&tmp_path)) {
                Ok(()) => {
                    log_in(
                        dir,
                        &format!(
                            "stale .tmp removed (the program file holds the same notes): tmp={} exe={}",
                            tmp_path.display(),
                            exe_path.display()
                        ),
                    );
                    true
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => true,
                Err(e) => {
                    log_in(
                        dir,
                        &format!(
                            "stale .tmp could not be removed, kept with its companion: tmp={} exe={} err={}",
                            tmp_path.display(),
                            exe_path.display(),
                            e
                        ),
                    );
                    false
                }
            };
            if !removed {
                return Ok(ins.decision);
            }
            if let Err(e) = remove_companion_in(dir, exe_path) {
                log_in(
                    dir,
                    &format!(
                        "companion of a stale .tmp could not be removed: file={} err={}",
                        companion_path.display(),
                        e
                    ),
                );
            }
        }
        PendingDecision::Conflict { cause, .. } => log_in(
            dir,
            &format!(
                "pending .tmp in conflict with the program file: tmp={} exe={} cause={}",
                tmp_path.display(),
                exe_path.display(),
                cause
            ),
        ),
        PendingDecision::Promote if ins.companion.is_none() => {
            // A .tmp left by a version without companion: from now on it is
            // judged like any other, against the notes the exe holds now.
            let exe = match &ins.exe {
                Some(Ok(e)) => e,
                _ => return Err("internal: the program file was not read for a .tmp to promote".to_string()),
            };
            let c = Companion {
                exe: exe_path.to_path_buf(),
                base: notes_sha(exe.image.payload.as_deref()),
                program_from_updater: false,
            };
            if let Err(e) = write_companion_in(dir, &c) {
                let msg = format!(
                    "companion of a pending .tmp left by an earlier version could not be written: tmp={} exe={} err={}",
                    tmp_path.display(),
                    exe_path.display(),
                    e
                );
                log_in(dir, &msg);
                return Err(msg);
            }
            log_in(
                dir,
                &format!(
                    "pending .tmp left by an earlier version taken up (companion written): tmp={} exe={}",
                    tmp_path.display(),
                    exe_path.display()
                ),
            );
        }
        PendingDecision::None | PendingDecision::Promote => {}
    }
    Ok(ins.decision)
}

// ── The session ──

/// What this process knows of the notes of the exe it runs from, from the
/// moment they are opened (`open_session`) to the exit.
///
/// The session keeps the encrypted payload it opened or last wrote. Reading
/// the notes again during the session (unlock after a lock, update) gives
/// that payload back, never what the files say at that moment: nothing
/// written by something else can replace the session's last save.
#[derive(Debug)]
pub struct Session {
    exe: PathBuf,
    /// SHA-256 of the notes the exe held when the session's starting point
    /// was written (the `base` of the companion).
    base: Option<Sha>,
    /// The encrypted payload the session opened or last wrote.
    current: Option<Vec<u8>>,
    /// Whether the .tmp, if any, holds what this session opened or wrote.
    tmp_is_ours: bool,
}

#[cfg(test)]
impl Session {
    /// The encrypted payload the session opened or last wrote (the process
    /// reads it through `read_session_data`).
    pub fn current(&self) -> Option<&[u8]> {
        self.current.as_deref()
    }
}

static SESSION: Mutex<Option<Session>> = Mutex::new(None);

fn session_guard() -> MutexGuard<'static, Option<Session>> {
    SESSION.lock().unwrap_or_else(|e| e.into_inner())
}

fn no_session(exe_path: &Path, open: Option<&Session>) -> String {
    match open {
        Some(s) => format!(
            "the notes open in this session belong to {}, not to {}",
            s.exe.display(),
            exe_path.display()
        ),
        None => format!("no notes are open for {}", exe_path.display()),
    }
}

/// Open the notes of `exe_path` for this process, after
/// `settle_pending_at_startup` (and the conflict question). Returns the
/// encrypted payload to unlock, or None when there are no notes yet.
pub fn open_session(exe_path: &Path) -> Result<Option<Vec<u8>>, String> {
    let (session, payload) = open_session_in(&staging_dir(), exe_path)?;
    *session_guard() = Some(session);
    Ok(payload)
}

/// `open_session` with the staging directory given by the caller, without
/// touching the session of the process.
pub fn open_session_in(dir: &Path, exe_path: &Path) -> Result<(Session, Option<Vec<u8>>), String> {
    let ins = inspect_in(dir, exe_path);
    match ins.decision {
        PendingDecision::Promote => {
            let c = match ins.companion {
                Some(Ok(c)) => c,
                _ => {
                    return Err(format!(
                        "the pending save {} has no companion file yet",
                        get_tmp_path_in(dir, exe_path).display()
                    ))
                }
            };
            let payload = match ins.tmp {
                Some(Ok(t)) => t.image.payload,
                _ => return Err("internal: a .tmp to promote that cannot be read".to_string()),
            };
            let session = Session {
                exe: exe_path.to_path_buf(),
                base: c.base,
                current: payload.clone(),
                tmp_is_ours: true,
            };
            Ok((session, payload))
        }
        PendingDecision::None | PendingDecision::Stale | PendingDecision::Ignored(_) => {
            let facts = match ins.exe {
                Some(r) => r,
                None => exe_facts(exe_path),
            }?;
            let payload = facts.image.payload;
            let session = Session {
                exe: exe_path.to_path_buf(),
                base: notes_sha(payload.as_deref()),
                current: payload.clone(),
                tmp_is_ours: false,
            };
            Ok((session, payload))
        }
        PendingDecision::Conflict { .. } => Err(format!(
            "two versions of the notes of {} are still in conflict",
            exe_path.display()
        )),
        PendingDecision::ExeUnreadable(e) | PendingDecision::TmpUnreadable(e) => Err(e),
    }
}

/// The encrypted payload of the open session of `exe_path`: what it opened
/// or last wrote (None: no notes yet). `Err` when no session of that exe is
/// open.
pub fn read_session_data(exe_path: &Path) -> Result<Option<Vec<u8>>, String> {
    let guard = session_guard();
    match guard.as_ref() {
        Some(s) if same_exe(&s.exe, exe_path) => Ok(s.current.clone()),
        other => Err(no_session(exe_path, other)),
    }
}

/// Read the notes of a LockNote file that is not the one of this session
/// (File > Import from another LockNote...): from its .tmp when that file is
/// the latest state (`PendingDecision::Promote`), from the exe otherwise.
/// Nothing is written. `Ok(None)` if no valid data is found.
///
/// `Err` (naming the file) when its .tmp or the companion of that .tmp
/// cannot be read (`PendingDecision::TmpUnreadable`): the .tmp may hold
/// notes more recent than the exe, which must not be given instead without
/// a word. A conflict still reads the exe (known limit, issue 055).
pub fn read_data(exe_path: &Path) -> Result<Option<Vec<u8>>, String> {
    read_data_in(&staging_dir(), exe_path)
}

/// `read_data` with the staging directory given by the caller.
pub fn read_data_in(dir: &Path, exe_path: &Path) -> Result<Option<Vec<u8>>, String> {
    let ins = inspect_in(dir, exe_path);
    match ins.decision {
        PendingDecision::Promote => match ins.tmp {
            Some(Ok(t)) => Ok(t.image.payload),
            _ => Err("internal: a .tmp to promote that cannot be read".to_string()),
        },
        PendingDecision::TmpUnreadable(e) => Err(e),
        PendingDecision::None
        | PendingDecision::Ignored(_)
        | PendingDecision::ExeUnreadable(_)
        | PendingDecision::Stale
        | PendingDecision::Conflict { .. } => Ok(read_payload_from_file(exe_path)),
    }
}

/// Before the session first writes the .tmp: a .tmp it did not open (one
/// that was ignored, or stale and not removable) is moved into `conflicts\`
/// rather than overwritten.
fn set_aside_foreign_tmp(dir: &Path, session: &Session) -> io::Result<()> {
    if session.tmp_is_ours {
        return Ok(());
    }
    let tmp_path = get_tmp_path_in(dir, &session.exe);
    match fs::symlink_metadata(&tmp_path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
        Ok(_) => {}
    }
    let (aside, f) = reserve_conflict_file(dir, &session.exe, "tmp")?;
    drop(f);
    if let Err(e) = pair_step("set tmp aside").and_then(|_| fs::rename(&tmp_path, &aside)) {
        drop_reserved(dir, &aside);
        return Err(e);
    }
    log_in(
        dir,
        &format!(
            "pending .tmp not opened by this session set aside before its first save: tmp={} aside={}",
            tmp_path.display(),
            aside.display()
        ),
    );
    Ok(())
}

/// Whether the .tmp of this session holds a program from the updater. The
/// companion of a .tmp the session wrote must be there and readable.
fn session_program_from_updater(dir: &Path, session: &Session) -> io::Result<bool> {
    if !session.tmp_is_ours {
        return Ok(false);
    }
    match read_companion_in(dir, &session.exe) {
        Some(Ok(c)) => Ok(c.program_from_updater),
        Some(Err(e)) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("the companion file of the pending save cannot be used: {}", e),
        )),
        None if fs::symlink_metadata(get_tmp_path_in(dir, &session.exe)).is_err() => Ok(false),
        None => Err(io::Error::new(
            io::ErrorKind::NotFound,
            "the companion file of the pending save has disappeared",
        )),
    }
}

/// Write the exe binary + marker + encrypted payload to the .tmp staging
/// file of the open session.
///
/// The program part is the one of the exe, unless the companion says that
/// the .tmp holds a program downloaded by the updater (then that one is
/// kept). Dates play no part. The companion keeps the base of the session.
/// The pair is written in its order (`stage_pair_in`), each file atomically
/// (`write_staging_file`): a failure leaves the previous .tmp as it was, and
/// is returned.
pub fn write_data(exe_path: &Path, encrypted: &[u8]) -> io::Result<()> {
    let mut guard = session_guard();
    match guard.as_mut() {
        Some(s) if same_exe(&s.exe, exe_path) => write_data_in(&staging_dir(), s, encrypted),
        other => Err(io::Error::new(io::ErrorKind::Other, no_session(exe_path, other.as_deref()))),
    }
}

/// `write_data` with the staging directory and the session given by the caller.
pub fn write_data_in(dir: &Path, session: &mut Session, encrypted: &[u8]) -> io::Result<()> {
    let tmp_path = get_tmp_path_in(dir, &session.exe);
    let from_updater = session_program_from_updater(dir, session)?;
    let program_source = if from_updater {
        fs::read(&tmp_path)?
    } else {
        fs::read(&session.exe)?
    };
    set_aside_foreign_tmp(dir, session)?;
    stage_pair_in(
        dir,
        &Companion { exe: session.exe.clone(), base: session.base, program_from_updater: from_updater },
        &build_image(&program_source, encrypted),
        None,
    )?;
    session.current = Some(encrypted.to_vec());
    session.tmp_is_ours = true;
    Ok(())
}

/// Stage a program downloaded by the updater for the exe of the open
/// session, followed by the session's notes (when there are any): the
/// companion records `program=updater`, so the exit promotes it even without
/// notes and later saves keep that program. Atomic, like `write_data`.
pub fn write_staged_update(exe_path: &Path, program: &[u8]) -> io::Result<()> {
    let mut guard = session_guard();
    match guard.as_mut() {
        Some(s) if same_exe(&s.exe, exe_path) => write_staged_update_in(&staging_dir(), s, program),
        other => Err(io::Error::new(io::ErrorKind::Other, no_session(exe_path, other.as_deref()))),
    }
}

/// `write_staged_update` with the staging directory and the session given
/// by the caller.
pub fn write_staged_update_in(dir: &Path, session: &mut Session, program: &[u8]) -> io::Result<()> {
    if find_marker(program).is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "the downloaded program already holds notes",
        ));
    }
    let image = match &session.current {
        Some(payload) => build_image(program, payload),
        None => program.to_vec(),
    };
    set_aside_foreign_tmp(dir, session)?;
    // Order of the pair: program=updater is written only once the .tmp
    // holds the updater's program (N1, issue 055 pass 3).
    stage_pair_in(
        dir,
        &Companion { exe: session.exe.clone(), base: session.base, program_from_updater: true },
        &image,
        None,
    )?;
    session.tmp_is_ours = true;
    Ok(())
}

/// Write the staging file at `path` through `write`, atomically: the bytes go
/// to a file of their own in the same directory (created exclusively), are
/// flushed to disk, and only then does that file take the name of the .tmp.
///
/// A failure at any step leaves the previous .tmp untouched, and never a
/// half-written one: the .tmp may be the only copy of the latest save, and
/// the swap helper moves it over the .exe as it is.
fn write_staging_file(
    path: &Path,
    write: impl FnOnce(&mut fs::File) -> io::Result<()>,
) -> io::Result<()> {
    pair_step("write staging file")?;
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "staging path has no file name"))?
        .to_string_lossy()
        .to_string();
    let part = path.with_file_name(format!(
        "{}.{}.{}",
        name,
        swap_common::random_id(),
        PART_EXTENSION
    ));

    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true) // never an existing file
        .open(&part)?;
    let written = write(&mut f).and_then(|_| f.sync_all());
    drop(f);

    let result = written.and_then(|_| fs::rename(&part, path));
    if result.is_err() {
        // The save has failed and is reported as such; the part file is
        // only debris. If it cannot be removed now, the startup cleanup
        // takes it (and the log says why it is there).
        if let Err(e) = fs::remove_file(&part) {
            if let Some(dir) = path.parent() {
                log_in(
                    dir,
                    &format!("part file could not be removed: file={} err={}", part.display(), e),
                );
            }
        }
    }
    result
}

// ── Conflict between a .tmp and the exe ──

/// Where the version set aside by a conflict is kept:
/// `%LOCALAPPDATA%\LockNote\conflicts`.
pub fn conflicts_dir() -> PathBuf {
    conflicts_dir_in(&staging_dir())
}

/// `conflicts_dir` with the staging directory given by the caller.
pub fn conflicts_dir_in(dir: &Path) -> PathBuf {
    dir.join(CONFLICTS_DIR)
}

/// Create, exclusively, the file that will hold the version set aside:
/// `<hash of the exe path>-<unix seconds>-<origin>.exe` (with a counter before
/// the origin when that name is taken). `origin` is `exe` or `tmp`.
///
/// Both origins are complete LockNote images, hence `.exe`: the file shows
/// up in "Import from another LockNote file", whose filter is `*.exe`.
fn reserve_conflict_file(dir: &Path, exe_path: &Path, origin: &str) -> io::Result<(PathBuf, fs::File)> {
    let conflicts = conflicts_dir_in(dir);
    fs::create_dir_all(&conflicts)?;
    let hash = swap_common::exe_hash(exe_path);
    let unix = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    for n in 0..1000u32 {
        let name = if n == 0 {
            format!("{}-{}-{}.exe", hash, unix, origin)
        } else {
            format!("{}-{}-{}-{}.exe", hash, unix, n, origin)
        };
        let path = conflicts.join(name);
        match fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(f) => return Ok((path, f)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!("no free name left in {}", conflicts.display()),
    ))
}

/// Remove a file reserved in `conflicts\` that did not receive its content.
fn drop_reserved(dir: &Path, path: &Path) {
    if let Err(e) = fs::remove_file(path) {
        log_in(
            dir,
            &format!("unfinished conflict file could not be removed: file={} err={}", path.display(), e),
        );
    }
}

fn no_conflict(decision: &PendingDecision) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("no conflict to resolve (state: {:?})", decision),
    )
}

/// Resolve a conflict by keeping the pending save (the .tmp).
///
/// The whole exe is first copied into `conflicts\` and read back. Unless it
/// holds a program from the updater, the .tmp is then staged again behind
/// the program of the exe (keeping its date); last, the companion takes the
/// notes the exe holds now as its base, so the normal rule applies: the
/// .tmp is opened at once and moved over the exe at exit. A failure at any
/// step leaves the conflict unresolved. Dates play no part in the decision.
/// The exe is not touched here. Returns the path of the copy of the exe.
pub fn resolve_conflict_keep_tmp(exe_path: &Path) -> io::Result<PathBuf> {
    resolve_conflict_keep_tmp_in(&staging_dir(), exe_path)
}

/// `resolve_conflict_keep_tmp` with the staging directory given by the caller.
pub fn resolve_conflict_keep_tmp_in(dir: &Path, exe_path: &Path) -> io::Result<PathBuf> {
    use std::io::Write;

    let ins = inspect_in(dir, exe_path);
    if !matches!(ins.decision, PendingDecision::Conflict { .. }) {
        return Err(no_conflict(&ins.decision));
    }
    let staged = match ins.tmp {
        Some(Ok(t)) => t,
        _ => return Err(io::Error::new(io::ErrorKind::InvalidData, "the .tmp in conflict cannot be read")),
    };
    let from_updater = matches!(ins.companion, Some(Ok(ref c)) if c.program_from_updater);

    let exe_data = fs::read(exe_path)?;

    // 1. Keep the whole exe aside, and check the copy.
    pair_step("copy exe aside")?;
    let (aside, mut f) = reserve_conflict_file(dir, exe_path, "exe")?;
    let written = f.write_all(&exe_data).and_then(|_| f.sync_all());
    drop(f);
    let checked = written.and_then(|_| {
        if fs::read(&aside)? == exe_data {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the copy of the program file does not read back identical",
            ))
        }
    });
    if let Err(e) = checked {
        drop_reserved(dir, &aside);
        return Err(e);
    }

    // 2. The pending save is now based on what the exe holds. Order of the
    //    pair: the .tmp is staged again first (still in conflict, its date
    //    kept), and only then does the companion take the new base, so a
    //    failure reported here leaves the conflict as it was (Reviewer 3,
    //    issue 055 pass 3).
    let c = Companion {
        exe: exe_path.to_path_buf(),
        base: notes_sha(exe_image(&exe_data).payload.as_deref()),
        program_from_updater: from_updater,
    };
    if from_updater {
        write_companion_in(dir, &c)?;
    } else {
        // A valid .tmp without the updater's program always has notes.
        let payload = staged.image.payload.as_deref().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "the .tmp in conflict has no notes")
        })?;
        stage_pair_in(dir, &c, &build_image(&exe_data, payload), Some(staged.modified))?;
    }

    // 3. The pending save must now be the one that is opened and promoted.
    match pending_decision_in(dir, exe_path) {
        PendingDecision::Promote => {
            log_in(
                dir,
                &format!(
                    "conflict resolved, pending save kept: exe={} program file copied to {}",
                    exe_path.display(),
                    aside.display()
                ),
            );
            Ok(aside)
        }
        other => Err(io::Error::new(
            io::ErrorKind::Other,
            format!("the pending save was taken up but is still not current (state: {:?})", other),
        )),
    }
}

/// Resolve a conflict by keeping the exe: the .tmp is moved into
/// `conflicts\` (not deleted), and its companion removed. Returns its new path.
pub fn resolve_conflict_keep_exe(exe_path: &Path) -> io::Result<PathBuf> {
    resolve_conflict_keep_exe_in(&staging_dir(), exe_path)
}

/// `resolve_conflict_keep_exe` with the staging directory given by the caller.
pub fn resolve_conflict_keep_exe_in(dir: &Path, exe_path: &Path) -> io::Result<PathBuf> {
    let decision = pending_decision_in(dir, exe_path);
    if !matches!(decision, PendingDecision::Conflict { .. }) {
        return Err(no_conflict(&decision));
    }
    // The name is reserved first (exclusive creation), then the .tmp takes
    // it: a rename only ever replaces the empty file created just above.
    let (aside, f) = reserve_conflict_file(dir, exe_path, "tmp")?;
    drop(f);
    if let Err(e) = pair_step("move tmp aside").and_then(|_| fs::rename(get_tmp_path_in(dir, exe_path), &aside)) {
        drop_reserved(dir, &aside);
        return Err(e);
    }
    if let Err(e) = remove_companion_in(dir, exe_path) {
        // Harmless: a companion without .tmp is removed at the next start.
        log_in(
            dir,
            &format!(
                "companion could not be removed after a conflict: file={} err={}",
                companion_path_in(dir, exe_path).display(),
                e
            ),
        );
    }
    log_in(
        dir,
        &format!(
            "conflict resolved, program file kept: exe={} pending save moved to {}",
            exe_path.display(),
            aside.display()
        ),
    );
    Ok(aside)
}

// ── Startup cleanup ──

/// Startup cleanup of what LockNote itself may have left behind, older than
/// a minute, regular files only, by exact name pattern:
///
/// - `<hash>.tmp.<id>.part` and `<hash>.base.<id>.part` of THIS exe in
///   %LOCALAPPDATA%\LockNote\ (a save interrupted before its rename);
/// - `locknote-swap-*.exe` in %TEMP% (extracted swap helpers: they either
///   finished their job or the swap attempt has long been abandoned);
/// - `<exe name>.<id>.lnswap` next to THIS exe (a cross-volume swap
///   interrupted between its copy and its rename).
///
/// It runs once the swap lock of this exe is held (`acquire_swap_lock`), so
/// no helper of this exe is at work. No `.tmp` is removed here, whichever exe
/// it belongs to: it may be the only copy of that exe's latest save. The .tmp
/// of this exe is handled by `settle_pending_at_startup`; the .tmp of another
/// exe is that exe's business.
pub fn cleanup_swap_leftovers() {
    let exe_path = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return,
    };
    cleanup_in(&staging_dir(), &std::env::temp_dir(), &exe_path);
}

/// `cleanup_swap_leftovers` with its directories given by the caller:
/// `staging` is the staging directory, `temp_dir` the directory the swap
/// helper is extracted to, `exe_path` the running exe.
fn cleanup_in(staging: &Path, temp_dir: &Path, exe_path: &Path) {
    // 1. Part files of this exe's staging file and companion
    for own in [get_tmp_path_in(staging, exe_path), companion_path_in(staging, exe_path)] {
        if let Some(own_name) = own.file_name().and_then(|n| n.to_str()) {
            remove_stale_files(staging, &|name| {
                swap_common::is_suffixed_name(name, own_name, PART_EXTENSION)
            });
        }
    }

    // 2. Stale extracted swap helpers in %TEMP%: "locknote-swap-*.exe"
    //    (pid and random suffix)
    remove_stale_files(temp_dir, &|name| {
        name.starts_with("locknote-swap-") && name.ends_with(".exe")
    });

    // 3. Copies left next to this exe by an interrupted cross-volume swap
    if let (Some(dir), Some(exe_name)) = (
        exe_path.parent(),
        exe_path.file_name().map(|n| n.to_string_lossy().to_string()),
    ) {
        remove_stale_files(dir, &|name| swap_common::is_sibling_name(name, &exe_name));
    }
}

/// Remove the regular files of `dir` whose name satisfies `matches` and that
/// are older than `CLEANUP_MIN_AGE`.
fn remove_stale_files(dir: &Path, matches: &dyn Fn(&str) -> bool) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if !matches(name) {
            continue;
        }

        // Use symlink_metadata + is_file to refuse following
        // symlinks/junctions. These directories are writable by any app
        // running as the user: without this, a local process could plant
        // `locknote-swap-x.exe -> C:\Users\me\Documents\taxes.pdf` and
        // have LockNote delete the target on next startup.
        let metadata = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if !metadata.file_type().is_file() {
            continue;
        }

        let modified = match metadata.modified() {
            Ok(t) => t,
            Err(_) => continue,
        };

        let age = SystemTime::now().duration_since(modified).unwrap_or_default();
        if age > CLEANUP_MIN_AGE {
            // Debris, not data: a file that resists is tried again at the
            // next start.
            let _ = fs::remove_file(&path);
        }
    }
}

// ── The swap lock ──

/// The swap lock of the exe this process runs from, held from startup until
/// the helper is queued on it at exit (`launch_swap_helper`) or until
/// `release_swap_lock`.
#[cfg(windows)]
static SWAP_LOCK: Mutex<Option<swap_common::SwapLock>> = Mutex::new(None);

/// Startup step, before anything of the notes is read or cleaned: wait (up
/// to `SWAP_LOCK_WAIT`) until no swap helper of `exe_path` is at work, and
/// keep that lock for the session. `Err`: a swap of this exe is still
/// running or stuck; nothing must be read or changed (logged).
#[cfg(windows)]
pub fn acquire_swap_lock(exe_path: &Path) -> Result<(), String> {
    use swap_common::{Obtained, SwapLock, SWAP_LOCK_WAIT};
    let name = swap_common::swap_lock_name(exe_path);
    match SwapLock::acquire(&name, SWAP_LOCK_WAIT) {
        Ok((lock, obtained)) => {
            if obtained == Obtained::Abandoned {
                log_swap_event(&format!(
                    "swap lock {} was abandoned by its previous owner (a session or a swap that ended \
                     without releasing it); the staging file is examined as usual: exe={}",
                    name,
                    exe_path.display()
                ));
            }
            *SWAP_LOCK.lock().unwrap_or_else(|e| e.into_inner()) = Some(lock);
            Ok(())
        }
        Err(e) => {
            let msg = format!("startup refused, swap lock not obtained: exe={} err={}", exe_path.display(), e);
            log_swap_event(&msg);
            Err(e)
        }
    }
}

#[cfg(not(windows))]
pub fn acquire_swap_lock(_exe_path: &Path) -> Result<(), String> {
    Err("unsupported platform".into())
}

/// Release the swap lock of this process, if it still holds it. Must run on
/// the thread that called `acquire_swap_lock` (the main thread).
pub fn release_swap_lock() {
    #[cfg(windows)]
    {
        let lock = SWAP_LOCK.lock().unwrap_or_else(|e| e.into_inner()).take();
        drop(lock);
    }
}

/// Set when the user answered Yes to "Restart LockNote now?" after an update
/// (`request_relaunch_after_swap`): the swap helper then starts the exe again
/// once it has replaced it.
static RELAUNCH_AFTER_SWAP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Ask the swap helper launched at exit to start LockNote again once the exe
/// is replaced. Without a swap at exit (nothing to promote), nothing restarts.
pub fn request_relaunch_after_swap() {
    RELAUNCH_AFTER_SWAP.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// Whether `request_relaunch_after_swap` was called in this process.
pub fn relaunch_requested() -> bool {
    RELAUNCH_AFTER_SWAP.load(std::sync::atomic::Ordering::SeqCst)
}

/// The log of the swap and of the exit step, in `staging_dir`.
pub const SWAP_LOG_NAME: &str = swap_common::SWAP_LOG_NAME;

/// The arguments of the swap helper, in the order it reads them
/// (`src/bin/locknote_swap.rs`, `parse_args`).
fn helper_args(pid: u32, tmp: &Path, exe: &Path, ready: &str, relaunch: bool) -> Vec<std::ffi::OsString> {
    let mut args: Vec<std::ffi::OsString> =
        vec![pid.to_string().into(), tmp.into(), exe.into(), ready.into()];
    if relaunch {
        args.push(swap_common::RELAUNCH_ARG.into());
    }
    args
}

/// Embedded companion helper binary. Built separately by `cargo build --bin
/// locknote-swap` and copied into `assets/` before compiling the main binary.
///
/// The helper waits for the parent process to exit and then performs the
/// .tmp → .exe replacement through the Win32 `MoveFileExW` API, which is
/// safer than the historical `cmd.exe /c ping & move` chain (no shell
/// injection surface, no cmd.exe dependency, no fixed-delay race condition).
/// When the exe is on another volume than the .tmp it copies the .tmp next
/// to the exe first (see `src/bin/locknote_swap.rs`).
#[cfg(windows)]
const SWAP_HELPER_BINARY: &[u8] =
    include_bytes!("../../assets/locknote-swap-embedded.exe");

/// Extract the embedded swap helper into %TEMP%, launch it detached so it can
/// survive the current process exiting, and hand the swap lock over to it.
///
/// The helper takes four arguments: `<parent_pid> <tmp_path> <exe_path>
/// <ready event>`, and a fifth, `--relaunch`, when the user asked to restart
/// after an update (`request_relaunch_after_swap`). It signals the event at the very moment it starts waiting
/// for the swap lock (`SignalObjectAndWait`); this process waits for that
/// signal (up to `HELPER_READY_WAIT`) and only then releases the lock, so the
/// helper owns it before any other LockNote of this exe can start. The helper
/// then waits for this process to exit and moves the .tmp over the exe.
///
/// On failure the helper leaves the .tmp in place and logs the error to
/// `swap-error.log`: the .tmp then remains the only copy of the latest save
/// (the exe still holds the previous one) until a later exit promotes it.
///
/// A late helper (issue #39): under load a new process can stall several
/// seconds before it runs. When the helper is not queued within
/// `HELPER_READY_WAIT`, or ends first, `Err` is returned and nothing is
/// promoted at this exit; the .tmp stays and is promoted at the next exit.
/// A helper still running then is ended (`end_late_helper`) before the lock
/// is released: it cannot hold the lock yet and touches nothing before it
/// does, and left alive it could reach the lock later, behind the next
/// session of this exe, and act for a session that is over (a move over an
/// exe still in use, a restart nobody asked for any more). Should it outlive
/// that step, it still never acts while a session holds the lock, never
/// after the 60 s of `SWAP_LOCK_WAIT`, and once it holds the lock it moves
/// only the .tmp present then, after taking the decision again on the files
/// as they are (`still_promotable`): never an older save over newer notes.
/// A helper that starts after this function closed the ready event cannot
/// open it and exits with code 3, nothing touched.
#[cfg(windows)]
pub fn launch_swap_helper(tmp_path: &Path, exe_path: &Path) -> Result<(), String> {
    use rand::Rng;
    use std::io::Write;
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    use windows::core::HSTRING;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::Threading::CreateEventW;

    // 1. Extract helper to a per-invocation path under %TEMP%.
    //
    // The filename carries 16 cryptographically-random bytes (128 bits) so a
    // concurrent local attacker cannot predict it and pre-plant a file at
    // the same path. We also open with create_new + share_mode=0 so the
    // write will fail if anything already exists there, instead of
    // overwriting it — this closes the TOCTOU between "choose a path" and
    // "write the helper bytes".
    let temp_dir = std::env::temp_dir();
    let pid = std::process::id();
    let mut rand_bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut rand_bytes);
    let mut rand_hex = String::with_capacity(32);
    for b in rand_bytes.iter() {
        rand_hex.push_str(&format!("{:02x}", b));
    }
    let helper_path = temp_dir.join(format!("locknote-swap-{}-{}.exe", pid, rand_hex));

    {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true) // fail if a file exists at this path
            .open(&helper_path)
            .map_err(|e| format!("extract helper: {}", e))?;
        f.write_all(SWAP_HELPER_BINARY)
            .map_err(|e| format!("extract helper: {}", e))?;
        f.sync_all()
            .map_err(|e| format!("extract helper: {}", e))?;
    }

    // 2. The event the helper signals once it is queued on the swap lock.
    let ready_name = format!("Local\\LockNote_SwapReady_{}", swap_common::random_id());
    let ready = unsafe { CreateEventW(None, true, false, &HSTRING::from(ready_name.as_str())) }
        .map_err(|e| format!("create ready event: {}", e))?;

    // 3. Spawn detached so we can exit immediately after.
    //    CREATE_NO_WINDOW hides the (nonexistent) console; DETACHED_PROCESS
    //    prevents inheriting our handles / console.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const DETACHED_PROCESS: u32 = 0x0000_0008;

    let relaunch = relaunch_requested();
    let spawned = Command::new(&helper_path)
        .args(helper_args(pid, tmp_path, exe_path, &ready_name, relaunch))
        .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
        .spawn();
    let child = match spawned {
        Ok(c) => c,
        Err(e) => {
            unsafe {
                let _ = CloseHandle(ready);
            }
            return Err(format!("spawn helper: {}", e));
        }
    };

    // 4. Hand the swap lock over once the helper is queued on it.
    let helper = HANDLE(std::os::windows::io::AsRawHandle::as_raw_handle(&child));
    let waited = wait_helper_ready(ready, helper, swap_common::HELPER_READY_WAIT);
    drop(child); // closes our handle; the helper runs on
    unsafe {
        let _ = CloseHandle(ready);
    }
    release_swap_lock();
    waited.map_err(|e| format!("{}; swap lock released", e))
}

/// Wait until the helper signals `ready` (it is queued on the swap lock), or
/// until its process `helper` ends first (it gave up: bad arguments, event
/// not found...), whichever comes first, at most `timeout`.
#[cfg(windows)]
fn wait_helper_ready(
    ready: windows::Win32::Foundation::HANDLE,
    helper: windows::Win32::Foundation::HANDLE,
    timeout: std::time::Duration,
) -> Result<(), String> {
    use windows::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForMultipleObjects};
    let millis = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX - 1);
    // Index 0 first: when both are signalled, the helper was queued.
    let waited = unsafe { WaitForMultipleObjects(&[ready, helper], false, millis) };
    if waited == WAIT_OBJECT_0 {
        return Ok(());
    }
    if waited.0 == WAIT_OBJECT_0.0 + 1 {
        let mut code = 0u32;
        let code = match unsafe { GetExitCodeProcess(helper, &mut code) } {
            Ok(()) => code.to_string(),
            Err(e) => format!("unknown: {}", e),
        };
        return Err(format!(
            "helper ended (exit code {}) before it was queued on the swap lock",
            code
        ));
    }
    if waited == WAIT_TIMEOUT {
        return Err(format!(
            "helper started but not queued on the swap lock within {} s; {}",
            timeout.as_secs(),
            end_late_helper(helper)
        ));
    }
    let failed = windows::core::Error::from_thread();
    Err(format!("waiting for the helper failed: {}; {}", failed, end_late_helper(helper)))
}

/// How long `end_late_helper` waits for the ended helper to be gone.
#[cfg(windows)]
const HELPER_END_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

/// End a helper that was not queued on the swap lock in time (issue #39),
/// before this process releases that lock. It cannot hold the lock (this
/// process does) and touches no file before it holds it, so ending it
/// touches nothing; left running, it could reach the lock later, behind the
/// next session of this exe, and act for a session that is over. The text
/// says what became of it, for the log.
#[cfg(windows)]
fn end_late_helper(helper: windows::Win32::Foundation::HANDLE) -> String {
    use windows::Win32::Foundation::WAIT_OBJECT_0;
    use windows::Win32::System::Threading::{TerminateProcess, WaitForSingleObject};
    let millis = u32::try_from(HELPER_END_WAIT.as_millis()).unwrap_or(u32::MAX - 1);
    // Exit code 3: the helper's own code for "swap lock not obtained,
    // nothing touched" (`src/bin/locknote_swap.rs`).
    if let Err(e) = unsafe { TerminateProcess(helper, 3) } {
        return format!("it could not be ended (it may still run): {}", e);
    }
    if unsafe { WaitForSingleObject(helper, millis) } != WAIT_OBJECT_0 {
        return format!("it was told to end but still ran after {} s (it may still run)", HELPER_END_WAIT.as_secs());
    }
    "it was ended, nothing touched (it held no lock)".to_string()
}

#[cfg(not(windows))]
pub fn launch_swap_helper(_tmp_path: &Path, _exe_path: &Path) -> Result<(), String> {
    Err("unsupported platform".into())
}

/// The end-of-process step, called once when `ui::run` returns, whatever the
/// way out (window closed, locked, too many wrong passwords, a dialog
/// cancelled before the editor): if the .tmp of `exe_path` is the latest
/// state (`PendingDecision::Promote`), launch the swap helper that moves it
/// over the exe once this process has exited.
///
/// It does not depend on a save having happened in this session: a .tmp left
/// by an earlier session whose swap failed is promoted too. A .tmp that does
/// not hold a program from the updater is first staged again behind the
/// program of the exe when the two programs differ, so a pending save never
/// brings an older program back. A .tmp that is ignored, stale, in conflict,
/// without companion yet, unreadable (or whose companion is), or whose exe
/// cannot be read, is not promoted (logged). A helper that cannot be extracted or started is logged to
/// `swap-error.log`; the .tmp stays in place.
///
/// True when a helper is queued (see `promote_pending_on_exit_in`).
pub fn promote_pending_on_exit(exe_path: &Path) -> bool {
    promote_pending_on_exit_in(&staging_dir(), exe_path, &launch_swap_helper)
}

/// `promote_pending_on_exit` with the staging directory and the launcher given
/// by the caller. True when a helper was launched and is queued on the swap
/// lock (it will replace the exe); false in every other case (logged).
fn promote_pending_on_exit_in(
    dir: &Path,
    exe_path: &Path,
    launch: &dyn Fn(&Path, &Path) -> Result<(), String>,
) -> bool {
    let tmp_path = get_tmp_path_in(dir, exe_path);
    let not_promoted = |why: &str| {
        log_in(
            dir,
            &format!(
                "pending .tmp not promoted ({}): tmp={} exe={}",
                why,
                tmp_path.display(),
                exe_path.display()
            ),
        );
        false
    };
    let ins = inspect_in(dir, exe_path);
    match &ins.decision {
        PendingDecision::Promote => {
            let companion = match &ins.companion {
                Some(Ok(c)) => c,
                _ => return not_promoted("no companion file yet; it is examined at the next start"),
            };
            let (staged, exe) = match (&ins.tmp, &ins.exe) {
                (Some(Ok(t)), Some(Ok(e))) => (t, e),
                _ => return not_promoted("internal: facts missing"),
            };
            if !companion.program_from_updater && staged.image.program != exe.image.program {
                // Same companion: the pair keeps what it claims.
                let restaged = match staged.image.payload.as_deref() {
                    Some(payload) => fs::read(exe_path).and_then(|exe_data| {
                        stage_pair_in(dir, companion, &build_image(&exe_data, payload), None)
                    }),
                    None => Err(io::Error::new(io::ErrorKind::InvalidData, "no notes")),
                };
                match restaged {
                    Ok(()) => log_in(
                        dir,
                        &format!(
                            "pending .tmp staged again behind the program of the exe (it held another program): tmp={} exe={}",
                            tmp_path.display(),
                            exe_path.display()
                        ),
                    ),
                    Err(e) => return not_promoted(&format!("staging it behind the program of the exe failed: {}", e)),
                }
            }
            if let Err(e) = launch(&tmp_path, exe_path) {
                log_in(
                    dir,
                    &format!(
                        "swap helper launch failed, .tmp kept: tmp={} exe={} err={}",
                        tmp_path.display(),
                        exe_path.display(),
                        e
                    ),
                );
                return false;
            }
            true
        }
        PendingDecision::Conflict { cause, .. } => {
            not_promoted(&format!("conflict with the program file: {}", cause))
        }
        PendingDecision::ExeUnreadable(e) | PendingDecision::TmpUnreadable(e) => not_promoted(e),
        PendingDecision::Ignored(reason) => not_promoted(&format!("ignored: {}", reason)),
        PendingDecision::None | PendingDecision::Stale => false,
    }
}

#[cfg(test)]
mod pending_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // ── Restart after an update ──

    #[test]
    fn the_helper_is_asked_to_restart_only_when_the_user_said_so() {
        let plain = helper_args(7, Path::new("a.tmp"), Path::new("LockNote.exe"), "Local\\r", false);
        assert_eq!(plain, ["7", "a.tmp", "LockNote.exe", "Local\\r"].map(std::ffi::OsString::from));
        let restart = helper_args(7, Path::new("a.tmp"), Path::new("LockNote.exe"), "Local\\r", true);
        assert_eq!(restart.len(), 5);
        assert_eq!(restart[4], swap_common::RELAUNCH_ARG, "last, after the ready event (the helper's order)");
    }

    // ── Marker tests ──

    #[test]
    fn marker_reconstruction_exact_bytes() {
        let marker = get_marker();
        let expected: [u8; 16] = [
            0x4C, 0x4E, 0x5F, 0x44, 0x41, 0x54, 0x41, 0x5F,
            0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xF0, 0x0D,
        ];
        assert_eq!(marker, expected);
    }

    #[test]
    fn marker_starts_with_ln_data() {
        let marker = get_marker();
        assert_eq!(&marker[..8], b"LN_DATA_");
    }

    #[test]
    fn get_marker_for_update_returns_correct_bytes() {
        let v = get_marker_for_update();
        assert_eq!(v.len(), 16);
        assert_eq!(v[0], 0x4C); // 'L'
        assert_eq!(v[7], 0x5F); // '_'
    }

    // ── Marker search tests ──

    #[test]
    fn find_marker_at_start() {
        let marker = get_marker();
        let mut data = marker.to_vec();
        data.extend_from_slice(&[0u8; 100]);
        assert_eq!(find_marker(&data), Some(0));
    }

    #[test]
    fn find_marker_at_end() {
        let marker = get_marker();
        let mut data = vec![0u8; 200];
        data.extend_from_slice(&marker);
        assert_eq!(find_marker(&data), Some(200));
    }

    #[test]
    fn find_marker_backward_finds_last_occurrence() {
        let marker = get_marker();
        let mut data = Vec::new();
        data.extend_from_slice(&marker); // at offset 0
        data.extend_from_slice(&[0u8; 50]);
        data.extend_from_slice(&marker); // at offset 66
        // Backward scan should find the LAST marker
        assert_eq!(find_marker(&data), Some(16 + 50));
    }

    #[test]
    fn find_marker_not_found() {
        let data = vec![0u8; 1000];
        assert_eq!(find_marker(&data), None);
    }

    #[test]
    fn find_marker_too_short() {
        let data = vec![0u8; 10];
        assert_eq!(find_marker(&data), None);
    }

    #[test]
    fn find_marker_empty() {
        let data: Vec<u8> = Vec::new();
        assert_eq!(find_marker(&data), None);
    }

    #[test]
    fn find_marker_exact_match_only() {
        // Partial marker should not match
        let marker = get_marker();
        let mut data = vec![0u8; 100];
        data.extend_from_slice(&marker[..15]); // only 15 bytes
        data.push(0xFF); // wrong last byte
        assert_eq!(find_marker(&data), None);
    }

    // ── tmp path tests ──

    #[test]
    fn tmp_path_is_deterministic() {
        let path = Path::new("C:\\Program Files\\LockNote\\LockNote.exe");
        let p1 = get_tmp_path(path);
        let p2 = get_tmp_path(path);
        assert_eq!(p1, p2);
    }

    #[test]
    fn tmp_path_differs_for_different_exe_paths() {
        let p1 = get_tmp_path(Path::new("C:\\dir1\\LockNote.exe"));
        let p2 = get_tmp_path(Path::new("C:\\dir2\\LockNote.exe"));
        assert_ne!(p1, p2);
    }

    #[test]
    fn tmp_path_case_insensitive() {
        let p1 = get_tmp_path(Path::new("C:\\Users\\Test\\LockNote.exe"));
        let p2 = get_tmp_path(Path::new("C:\\USERS\\TEST\\LOCKNOTE.EXE"));
        let p3 = get_tmp_path(Path::new("c:\\users\\test\\locknote.exe"));
        assert_eq!(p1, p2);
        assert_eq!(p2, p3);
    }

    #[test]
    fn tmp_path_ends_with_tmp_extension() {
        let p = get_tmp_path(Path::new("C:\\test\\app.exe"));
        assert_eq!(p.extension().unwrap(), "tmp");
    }

    #[test]
    fn tmp_path_filename_is_16_hex_chars() {
        let p = get_tmp_path(Path::new("C:\\test\\app.exe"));
        let stem = p.file_stem().unwrap().to_str().unwrap();
        assert_eq!(stem.len(), 16);
        assert!(stem.chars().all(|c| c.is_ascii_hexdigit()));
        // Must be lowercase
        assert_eq!(stem, stem.to_lowercase());
    }

    /// A fresh directory of its own: unique per process and per call, so
    /// that several `cargo test` processes never share a path.
    fn scratch(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "locknote_storage_{}_{}_{}",
            tag,
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    // ── read_data tests ──

    #[test]
    fn read_data_no_marker_returns_none() {
        let dir = scratch("no_marker");
        let file = dir.join("test_no_marker.exe");
        fs::write(&file, &[0u8; 500]).unwrap();

        let result = read_payload_from_file(&file);
        assert!(result.is_none());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_data_payload_too_small_returns_none() {
        let dir = scratch("small_payload");
        let file = dir.join("test_small.exe");

        let marker = get_marker();
        let mut data = vec![0u8; 100]; // exe stub
        data.extend_from_slice(&marker);
        data.extend_from_slice(&[0xAB; 50]); // less than MIN_PAYLOAD_SIZE (80)
        fs::write(&file, &data).unwrap();

        let result = read_payload_from_file(&file);
        assert!(result.is_none());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_data_nonexistent_file_returns_none() {
        let result = read_payload_from_file(Path::new("C:\\nonexistent\\file.exe"));
        assert!(result.is_none());
    }

    // ── Round-trip write/read test ──

    #[test]
    fn roundtrip_write_then_read() {
        let dir = scratch("roundtrip");
        let staging = dir.join("staging");
        let exe_file = dir.join("roundtrip.exe");

        // Create a fake exe (no marker)
        let exe_stub = vec![0x4Du8, 0x5A, 0x90, 0x00]; // MZ header stub
        fs::write(&exe_file, &exe_stub).unwrap();

        // Fake encrypted payload (>= 80 bytes)
        let payload: Vec<u8> = (0..100).map(|i| (i as u8).wrapping_mul(7)).collect();

        // Open (no notes yet), then write
        let (mut session, opened) = open_session_in(&staging, &exe_file).unwrap();
        assert_eq!(opened, None);
        write_data_in(&staging, &mut session, &payload).unwrap();

        // The tmp file should now exist
        let tmp_path = get_tmp_path_in(&staging, &exe_file);
        assert!(tmp_path.exists());

        // Read from tmp
        let recovered = read_payload_from_file(&tmp_path).unwrap();
        assert_eq!(recovered, payload);

        // The session gives back what it wrote; another reader finds it via the tmp
        assert_eq!(session.current(), Some(&payload[..]));
        let via_read_data = read_data_in(&staging, &exe_file).unwrap().unwrap();
        assert_eq!(via_read_data, payload);

        // Verify the tmp file structure: [exe_stub][marker][payload]
        let raw = fs::read(&tmp_path).unwrap();
        assert_eq!(&raw[..exe_stub.len()], &exe_stub);
        assert_eq!(&raw[exe_stub.len()..exe_stub.len() + MARKER_LEN], &get_marker());
        assert_eq!(&raw[exe_stub.len() + MARKER_LEN..], &payload[..]);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_strips_old_marker_and_payload() {
        let dir = scratch("strip_old");
        let staging = dir.join("staging");
        let exe_file = dir.join("strip_old.exe");

        // Create exe with existing marker + old payload
        let exe_stub = vec![0xCCu8; 200];
        let marker = get_marker();
        let old_payload = vec![0xBB; 100];
        let mut full = exe_stub.clone();
        full.extend_from_slice(&marker);
        full.extend_from_slice(&old_payload);
        fs::write(&exe_file, &full).unwrap();

        // Write new payload
        let (mut session, opened) = open_session_in(&staging, &exe_file).unwrap();
        assert_eq!(opened, Some(old_payload));
        let new_payload: Vec<u8> = (0..120).collect();
        write_data_in(&staging, &mut session, &new_payload).unwrap();

        // Read back: should get new payload, not old
        let tmp_path = get_tmp_path_in(&staging, &exe_file);
        let raw = fs::read(&tmp_path).unwrap();

        // Clean exe portion should be exactly the stub (200 bytes)
        assert_eq!(raw.len(), 200 + MARKER_LEN + new_payload.len());
        assert_eq!(&raw[..200], &exe_stub[..]);

        let recovered = read_payload_from_file(&tmp_path).unwrap();
        assert_eq!(recovered, new_payload);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_preserves_staged_update_binary() {
        // The updater stages a new binary in the .tmp, then the user saves
        // before closing. write_data must keep the new binary prefix from
        // the .tmp instead of re-reading the old .exe.
        let dir = scratch("preserve_update");
        let staging = dir.join("staging");
        let exe_file = dir.join("preserve_update.exe");
        let tmp_path = get_tmp_path_in(&staging, &exe_file);

        let old_exe = vec![0xAAu8; 128];
        let marker = get_marker();
        let staged_payload: Vec<u8> = (0..100).map(|i| i as u8).collect();
        let mut exe_bytes = old_exe.clone();
        exe_bytes.extend_from_slice(&marker);
        exe_bytes.extend_from_slice(&staged_payload);
        fs::write(&exe_file, &exe_bytes).unwrap();

        let (mut session, _) = open_session_in(&staging, &exe_file).unwrap();
        let new_exe = vec![0xBBu8; 192];
        write_staged_update_in(&staging, &mut session, &new_exe).unwrap();

        let fresh_payload: Vec<u8> = (0..120).map(|i| (i as u8).wrapping_mul(3)).collect();
        write_data_in(&staging, &mut session, &fresh_payload).unwrap();

        let raw = fs::read(&tmp_path).unwrap();
        assert_eq!(&raw[..new_exe.len()], &new_exe[..],
            ".tmp prefix must be the staged NEW binary, not the old .exe");
        assert_eq!(&raw[new_exe.len()..new_exe.len() + MARKER_LEN], &marker);
        assert_eq!(&raw[new_exe.len() + MARKER_LEN..], &fresh_payload[..]);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_never_takes_the_program_of_a_tmp_the_updater_did_not_stage() {
        // A pending .tmp holds an older program (written by an older version
        // of LockNote) and is dated after the exe. The program of a save is
        // the one of the exe whatever the dates: only a program staged by the
        // updater is kept from the .tmp.
        let dir = scratch("program_of_exe");
        let staging = dir.join("staging");
        let exe_file = dir.join("program_of_exe.exe");
        let tmp_path = get_tmp_path_in(&staging, &exe_file);
        fs::create_dir_all(&staging).unwrap();

        let marker = get_marker();
        let exe_payload: Vec<u8> = (0..100).map(|i| (i as u8).wrapping_add(9)).collect();
        let mut exe_bytes = vec![0x22u8; 192];
        exe_bytes.extend_from_slice(&marker);
        exe_bytes.extend_from_slice(&exe_payload);
        fs::write(&exe_file, &exe_bytes).unwrap();
        fs::OpenOptions::new().write(true).open(&exe_file).unwrap()
            .set_modified(SystemTime::now() - std::time::Duration::from_secs(3600)).unwrap();

        let pending: Vec<u8> = (0..100).map(|i| i as u8).collect();
        let mut stale_tmp = vec![0x11u8; 128];
        stale_tmp.extend_from_slice(&marker);
        stale_tmp.extend_from_slice(&pending);
        fs::write(&tmp_path, &stale_tmp).unwrap();
        write_companion_in(&staging, &Companion {
            exe: exe_file.clone(),
            base: Some(sha(&exe_payload)),
            program_from_updater: false,
        }).unwrap();

        let (mut session, opened) = open_session_in(&staging, &exe_file).unwrap();
        assert_eq!(opened, Some(pending), "the pending save is the latest state");
        let fresh_payload: Vec<u8> = (0..120).map(|i| (i as u8).wrapping_mul(3)).collect();
        write_data_in(&staging, &mut session, &fresh_payload).unwrap();

        let raw = fs::read(&tmp_path).unwrap();
        assert_eq!(&raw[..192], &exe_bytes[..192],
            ".tmp prefix must be the program of the .exe, not the one of the older .tmp");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_sets_an_unusable_tmp_aside_and_uses_the_exe_program() {
        // A .tmp without a valid marker (truncated / corrupted) is not the
        // program source, and the first save does not overwrite it: it is
        // moved into conflicts\ first.
        let dir = scratch("corrupt_tmp");
        let staging = dir.join("staging");
        let exe_file = dir.join("corrupt_tmp.exe");
        let tmp_path = get_tmp_path_in(&staging, &exe_file);
        fs::create_dir_all(&staging).unwrap();

        let exe = vec![0xCCu8; 96];
        fs::write(&exe_file, &exe).unwrap();
        // Garbage without the marker
        fs::write(&tmp_path, &[0xFFu8; 64]).unwrap();

        let (mut session, opened) = open_session_in(&staging, &exe_file).unwrap();
        assert_eq!(opened, None, "the session opens on the exe");
        let payload: Vec<u8> = (0..100).collect();
        write_data_in(&staging, &mut session, &payload).unwrap();

        let raw = fs::read(&tmp_path).unwrap();
        assert_eq!(&raw[..exe.len()], &exe[..],
            "corrupt .tmp must not be the program source; .exe binary used");
        let aside: Vec<PathBuf> = fs::read_dir(conflicts_dir_in(&staging)).unwrap()
            .flatten().map(|e| e.path()).collect();
        assert_eq!(aside.len(), 1);
        assert_eq!(fs::read(&aside[0]).unwrap(), vec![0xFFu8; 64], "the unusable .tmp is kept aside");

        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn swap_helper_binary_is_embedded() {
        // The embedded helper must be present and non-trivial in size.
        // A valid PE file is at least a few hundred bytes; we enforce >= 4 KB
        // to catch a truncated or placeholder asset.
        assert!(
            SWAP_HELPER_BINARY.len() >= 4096,
            "embedded swap helper looks too small ({} bytes)",
            SWAP_HELPER_BINARY.len()
        );
        // PE files start with the MZ magic "MZ" (0x4D 0x5A).
        assert_eq!(
            &SWAP_HELPER_BINARY[..2],
            b"MZ",
            "embedded swap helper is not a valid PE (missing MZ header)"
        );
    }

    // ── Legacy hint prefix tests (backward compatibility) ──

    #[test]
    fn split_hint_no_magic_returns_empty_hint() {
        let data = vec![0xAB; 100];
        let (hint, encrypted) = split_hint_and_encrypted(&data);
        assert_eq!(hint, "");
        assert_eq!(encrypted, data);
    }

    #[test]
    fn split_hint_empty_payload() {
        let data: Vec<u8> = Vec::new();
        let (hint, encrypted) = split_hint_and_encrypted(&data);
        assert_eq!(hint, "");
        assert!(encrypted.is_empty());
    }

    #[test]
    fn old_format_backward_compatible() {
        // Simulate old-format payload (no hint magic, starts with random salt)
        let mut old_payload = vec![0x12, 0x34, 0x56, 0x78]; // random salt bytes
        old_payload.extend_from_slice(&[0u8; 96]); // rest of encrypted data
        let (hint, encrypted) = split_hint_and_encrypted(&old_payload);
        assert_eq!(hint, "");
        assert_eq!(encrypted, old_payload);
    }

    #[test]
    fn split_hint_truncated_header() {
        // Less than 6 bytes — can't contain valid hint header
        let data = vec![b'H', b'N', b'T', 0x00, 0x05]; // 5 bytes, missing hint_len high byte
        let (hint, encrypted) = split_hint_and_encrypted(&data);
        assert_eq!(hint, "");
        assert_eq!(encrypted, data);
    }

    #[test]
    fn split_hint_at_max_length() {
        // Hint exactly at MAX_HINT_LEN (100 bytes)
        let hint_text = "A".repeat(100);
        let mut data = Vec::new();
        data.extend_from_slice(b"HNT\x00");
        data.extend_from_slice(&100u16.to_le_bytes());
        data.extend_from_slice(hint_text.as_bytes());
        data.extend_from_slice(&[0xAB; 50]); // encrypted payload
        let (hint, encrypted) = split_hint_and_encrypted(&data);
        assert_eq!(hint, hint_text);
        assert_eq!(encrypted.len(), 50);
    }

    #[test]
    fn split_hint_exceeds_max_length() {
        // Hint length > MAX_HINT_LEN — treated as invalid, fallback to no hint
        let mut data = Vec::new();
        data.extend_from_slice(b"HNT\x00");
        data.extend_from_slice(&101u16.to_le_bytes()); // 101 > MAX_HINT_LEN
        data.extend_from_slice(&[0x41; 101]);
        data.extend_from_slice(&[0xAB; 50]);
        let (hint, encrypted) = split_hint_and_encrypted(&data);
        assert_eq!(hint, "");
        assert_eq!(encrypted, data); // entire payload returned as encrypted
    }

    #[test]
    fn split_hint_length_exceeds_payload() {
        // Hint says 50 bytes but payload only has 10 bytes after header
        let mut data = Vec::new();
        data.extend_from_slice(b"HNT\x00");
        data.extend_from_slice(&50u16.to_le_bytes());
        data.extend_from_slice(&[0x41; 10]); // only 10 bytes, not 50
        let (hint, encrypted) = split_hint_and_encrypted(&data);
        assert_eq!(hint, "");
        assert_eq!(encrypted, data);
    }

    #[test]
    fn tmp_path_deterministic_same_input() {
        let path = std::path::Path::new("C:\\test\\locknote.exe");
        let p1 = get_tmp_path(path);
        let p2 = get_tmp_path(path);
        assert_eq!(p1, p2);
    }

    #[test]
    fn write_and_read_roundtrip() {
        let dir = scratch("storage");
        let staging = dir.join("staging");
        let exe_path = dir.join("test_roundtrip.exe");

        // Create a fake exe (no marker)
        let mut exe_data = vec![0x4D, 0x5A]; // MZ header stub
        exe_data.extend_from_slice(&[0u8; 100]); // padding
        std::fs::write(&exe_path, &exe_data).unwrap();

        // Write encrypted payload
        let payload: Vec<u8> = (0..100).map(|i| (i * 7 + 3) as u8).collect();
        let (mut session, _) = open_session_in(&staging, &exe_path).unwrap();
        write_data_in(&staging, &mut session, &payload).unwrap();

        // The tmp file should exist
        let tmp_path = get_tmp_path_in(&staging, &exe_path);
        assert!(tmp_path.exists());

        // Read it back — should prefer tmp
        let read_back = read_data_in(&staging, &exe_path).unwrap();
        assert!(read_back.is_some());
        assert_eq!(read_back.unwrap(), payload);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
