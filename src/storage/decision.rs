// What is done with a pending staging file: the LockNote image format (the
// marker), the companion file of a `.tmp`, the facts read from the `.tmp` and
// from the exe, and the decision taken from those facts (`decide_pending`).
//
// This file is compiled into both binaries: `storage` declares it as a
// module, and `src/bin/locknote_swap.rs` includes it with `#[path]`, so the
// helper re-checks, right before it renames anything over the exe, the very
// decision the application took at exit. Each binary uses only a part of it.

use sha2::{Digest, Sha256};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// XOR key used to obfuscate the marker in the binary.
const XOR_KEY: u8 = 0xAA;

/// Minimum valid payload size: salt(16) + iv(16) + hmac(32) + one AES block(16).
pub const MIN_PAYLOAD_SIZE: usize = 80;

/// Marker length in bytes.
pub const MARKER_LEN: usize = 16;

/// First line of a companion file.
const COMPANION_HEADER: &str = "LockNote staging companion 1";

/// The marker bytes stored XORed so the raw sentinel never appears in the binary.
/// Raw:    4C 4E 5F 44 41 54 41 5F DE AD BE EF CA FE F0 0D
/// Stored: each byte XORed with 0xAA
const MARKER_XORED: [u8; MARKER_LEN] = [
    0x4C ^ XOR_KEY, 0x4E ^ XOR_KEY, 0x5F ^ XOR_KEY, 0x44 ^ XOR_KEY,
    0x41 ^ XOR_KEY, 0x54 ^ XOR_KEY, 0x41 ^ XOR_KEY, 0x5F ^ XOR_KEY,
    0xDE ^ XOR_KEY, 0xAD ^ XOR_KEY, 0xBE ^ XOR_KEY, 0xEF ^ XOR_KEY,
    0xCA ^ XOR_KEY, 0xFE ^ XOR_KEY, 0xF0 ^ XOR_KEY, 0x0D ^ XOR_KEY,
];

/// Reconstruct the 16-byte binary marker by XORing with the key.
pub fn get_marker() -> [u8; MARKER_LEN] {
    let mut marker = [0u8; MARKER_LEN];
    for i in 0..MARKER_LEN {
        marker[i] = MARKER_XORED[i] ^ XOR_KEY;
    }
    marker
}

/// Backward linear scan from EOF to find the marker.
/// Returns the byte offset of the marker start, or None if not found.
pub fn find_marker(data: &[u8]) -> Option<usize> {
    let marker = get_marker();
    if data.len() < MARKER_LEN {
        return None;
    }
    for i in (0..=(data.len() - MARKER_LEN)).rev() {
        if data[i..i + MARKER_LEN] == marker {
            return Some(i);
        }
    }
    None
}

/// A SHA-256 digest.
pub type Sha = [u8; 32];

pub fn sha(bytes: &[u8]) -> Sha {
    Sha256::digest(bytes).into()
}

/// The digest that stands for "these notes": None when there are none.
pub fn notes_sha(payload: Option<&[u8]>) -> Option<Sha> {
    payload.map(sha)
}

fn to_hex(d: &Sha) -> String {
    d.iter().map(|b| format!("{:02x}", b)).collect()
}

fn from_hex(s: &str) -> Option<Sha> {
    if s.len() != 64 || !s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
        let pair = std::str::from_utf8(chunk).ok()?;
        out[i] = u8::from_str_radix(pair, 16).ok()?;
    }
    Some(out)
}

/// Whether two exe paths name the same file for LockNote (the comparison the
/// staging file name is built on).
pub fn same_exe(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().to_uppercase() == b.to_string_lossy().to_uppercase()
}

// ── The companion of a .tmp ──

/// What is recorded next to a pending .tmp, in `{hash}.base` (text, one
/// `key=value` per line, written atomically like the .tmp):
///
/// ```text
/// LockNote staging companion 1
/// exe=<full path of the exe>
/// base=<SHA-256 of the notes of the exe the session started from | none>
/// program=<exe | updater>
/// ```
///
/// `base` is what makes the decision independent of dates: as long as the
/// exe still holds those notes, nothing else has written it since the
/// session started, and the .tmp is the latest state. `program=updater`
/// says that the program part of the .tmp was downloaded by the updater and
/// must be kept; otherwise the program of the exe is the one that counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Companion {
    pub exe: PathBuf,
    /// SHA-256 of the payload the session was based on; None when the exe
    /// held no notes.
    pub base: Option<Sha>,
    pub program_from_updater: bool,
}

impl Companion {
    pub fn to_text(&self) -> String {
        self.to_text_with_volume(None)
    }

    /// The text, with the optional `volume=<8 hex>` line: the serial number
    /// of the volume the exe was on when the companion was written. Only the
    /// "Temporary files" dialog reads it (`storage::inventory`, issue 057): a
    /// missing exe in a folder that exists is an orphan only on that volume.
    pub fn to_text_with_volume(&self, volume: Option<u32>) -> String {
        let mut text = format!(
            "{}\nexe={}\nbase={}\nprogram={}\n",
            COMPANION_HEADER,
            self.exe.display(),
            self.base.as_ref().map(to_hex).unwrap_or_else(|| "none".to_string()),
            if self.program_from_updater { "updater" } else { "exe" }
        );
        if let Some(v) = volume {
            text.push_str(&format!("volume={:08x}\n", v));
        }
        text
    }

    /// Strict reading: the header, then exactly the three keys, once each,
    /// and at most one `volume` line (ignored here, see `parse_with_volume`).
    pub fn parse(text: &str) -> Result<Companion, String> {
        Self::parse_with_volume(text).map(|(c, _)| c)
    }

    /// `parse`, with the volume serial number when the line is there.
    pub fn parse_with_volume(text: &str) -> Result<(Companion, Option<u32>), String> {
        let mut lines = text.lines();
        if lines.next() != Some(COMPANION_HEADER) {
            return Err("unknown header".to_string());
        }
        let (mut exe, mut base, mut program, mut volume) = (None, None, None, None);
        for line in lines {
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| format!("line without '=': {:?}", line))?;
            let slot = match key {
                "exe" => &mut exe,
                "base" => &mut base,
                "program" => &mut program,
                "volume" => &mut volume,
                _ => return Err(format!("unknown key {:?}", key)),
            };
            if slot.replace(value.to_string()).is_some() {
                return Err(format!("key {:?} given twice", key));
            }
        }
        let exe = exe.filter(|e| !e.is_empty()).ok_or("missing key \"exe\"")?;
        let base = match base.as_deref() {
            Some("none") => None,
            Some(h) => Some(from_hex(h).ok_or_else(|| format!("invalid base {:?}", h))?),
            None => return Err("missing key \"base\"".to_string()),
        };
        let program_from_updater = match program.as_deref() {
            Some("exe") => false,
            Some("updater") => true,
            Some(other) => return Err(format!("invalid program {:?}", other)),
            None => return Err("missing key \"program\"".to_string()),
        };
        let volume = match volume.as_deref() {
            None => None,
            Some(v) if v.len() == 8 && v.bytes().all(|b| b.is_ascii_hexdigit()) => {
                Some(u32::from_str_radix(v, 16).map_err(|_| format!("invalid volume {:?}", v))?)
            }
            Some(v) => return Err(format!("invalid volume {:?}", v)),
        };
        Ok((Companion { exe: PathBuf::from(exe), base, program_from_updater }, volume))
    }
}

/// Why a companion file that exists cannot be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompanionError {
    /// It cannot be read (an I/O error: held without sharing by an
    /// antivirus or a backup tool, access denied...), or it is not a regular
    /// file. It says nothing about the .tmp: `decide_pending` turns it into
    /// `PendingDecision::TmpUnreadable`, never into a conflict. The text
    /// names the file.
    Unreadable(String),
    /// It was read, but its content is not usable (malformed, or written for
    /// another exe): a conflict (`ConflictCause::CompanionInvalid`).
    Invalid(String),
}

impl fmt::Display for CompanionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompanionError::Unreadable(e) | CompanionError::Invalid(e) => write!(f, "{}", e),
        }
    }
}

/// The companion file at `path`, for the exe `exe_path`: None when there is
/// no such file, `Some(Err)` when it exists but cannot be read
/// (`CompanionError::Unreadable`) or its content cannot be used
/// (`CompanionError::Invalid`).
pub fn read_companion(path: &Path, exe_path: &Path) -> Option<Result<Companion, CompanionError>> {
    let unreadable = |e: String| {
        Some(Err(CompanionError::Unreadable(format!(
            "the companion file {} of the pending save cannot be read: {}",
            path.display(),
            e
        ))))
    };
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_file() => {}
        Ok(_) => return unreadable("not a regular file".to_string()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return None,
        Err(e) => return unreadable(e.to_string()),
    }
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return None,
        Err(e) => return unreadable(e.to_string()),
    };
    let parsed = String::from_utf8(bytes)
        .map_err(|_| "not UTF-8 text".to_string())
        .and_then(|text| Companion::parse(&text))
        .and_then(|c| {
            if same_exe(&c.exe, exe_path) {
                Ok(c)
            } else {
                Err(format!("written for another program file ({})", c.exe.display()))
            }
        });
    Some(parsed.map_err(CompanionError::Invalid))
}

// ── What is done with an existing .tmp ──

/// A LockNote file as far as the decision is concerned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// SHA-256 of the program part: everything before the marker, or the
    /// whole file when it has none.
    pub program: Sha,
    /// The notes after the marker; None when there is no marker.
    pub payload: Option<Vec<u8>>,
}

/// A staged image. A marker followed by fewer bytes than the smallest
/// encrypted note is not usable.
pub fn staged_image(data: &[u8]) -> Result<Image, IgnoreReason> {
    match find_marker(data) {
        None => Ok(Image { program: sha(data), payload: None }),
        Some(pos) => {
            let payload = &data[pos + MARKER_LEN..];
            if payload.len() < MIN_PAYLOAD_SIZE {
                return Err(IgnoreReason::PayloadTooShort(payload.len()));
            }
            Ok(Image { program: sha(&data[..pos]), payload: Some(payload.to_vec()) })
        }
    }
}

/// The exe as an image: a payload too short to be a note counts as none
/// (as `read_payload_from_file` reads it).
pub fn exe_image(data: &[u8]) -> Image {
    match find_marker(data) {
        None => Image { program: sha(data), payload: None },
        Some(pos) => {
            let payload = &data[pos + MARKER_LEN..];
            Image {
                program: sha(&data[..pos]),
                payload: (payload.len() >= MIN_PAYLOAD_SIZE).then(|| payload.to_vec()),
            }
        }
    }
}

/// Why a .tmp is left alone: neither promoted nor deleted, and no session is
/// opened on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IgnoreReason {
    /// No marker, and not a program staged by the updater: not a LockNote
    /// image (a write cut short by an older version).
    NoMarker,
    /// A marker, but fewer bytes after it than the smallest encrypted note.
    PayloadTooShort(usize),
    /// The .tmp exists but cannot be read (or is not a regular file). A fact
    /// only: `decide_pending` turns it into `PendingDecision::TmpUnreadable`,
    /// never into `Ignored` (what cannot be read may be the latest save).
    Unreadable(String),
}

impl fmt::Display for IgnoreReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IgnoreReason::NoMarker => write!(f, "no marker"),
            IgnoreReason::PayloadTooShort(len) => write!(f, "payload too short ({} bytes)", len),
            IgnoreReason::Unreadable(e) => write!(f, ".tmp unreadable ({})", e),
        }
    }
}

/// Why a .tmp and the exe are in conflict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictCause {
    /// The exe no longer holds the notes the pending save was based on: it
    /// was written by something else since (another computer, a copy, a
    /// manual update).
    ExeChanged,
    /// The companion file was read but its content cannot be used
    /// (`CompanionError::Invalid`). One that cannot be read at all is not a
    /// conflict: `PendingDecision::TmpUnreadable`.
    CompanionInvalid(String),
    /// A .tmp left by a version without companion file, and the exe is more
    /// recent (the only case judged by dates).
    LegacyExeNewer,
}

impl fmt::Display for ConflictCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConflictCause::ExeChanged => write!(f, "the program file no longer holds the notes the pending save was based on"),
            ConflictCause::CompanionInvalid(e) => write!(f, "companion file unusable: {}", e),
            ConflictCause::LegacyExeNewer => write!(f, ".tmp without companion, program file more recent"),
        }
    }
}

/// What to do with the .tmp of an exe. The same decision is taken at startup
/// (`settle_pending_at_startup`), when opening the session
/// (`open_session`), when reading another LockNote file (`read_data`) and
/// when the process exits (`promote_pending_on_exit`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingDecision {
    /// No .tmp.
    None,
    /// The .tmp holds no usable image (no marker, payload too short): it is
    /// neither promoted nor deleted, the reason is logged, and the session
    /// opens on the exe.
    Ignored(IgnoreReason),
    /// The .tmp, or its companion file, exists but cannot be read, or is not
    /// a regular file: the .tmp may be the latest save, and without its
    /// companion nothing tells what it was based on, so nothing can be
    /// decided. Nothing is promoted or set aside and no session is opened
    /// (message naming the file, then exit).
    TmpUnreadable(String),
    /// There is a .tmp but the exe cannot be read: nothing can be decided.
    /// Nothing is promoted and no session is opened.
    ExeUnreadable(String),
    /// The exe holds the same notes as the .tmp (and the .tmp holds no
    /// program from the updater): the .tmp holds nothing more. It is deleted.
    Stale,
    /// The exe holds other notes than the ones the pending save was based
    /// on: only the user can tell which one to keep. Nothing is promoted or
    /// read from the .tmp until `resolve_conflict_keep_tmp` or
    /// `resolve_conflict_keep_exe` has run.
    Conflict {
        tmp_modified: SystemTime,
        exe_modified: SystemTime,
        /// false when the exe holds no notes at all (a clean program).
        exe_has_data: bool,
        cause: ConflictCause,
    },
    /// The .tmp is the latest state: the session opens on it, and it is
    /// moved over the exe when the process exits.
    Promote,
}

/// What `decide_pending` needs to know about a .tmp that exists and can be read.
#[derive(Debug, Clone)]
pub struct StagedFacts {
    pub image: Image,
    pub modified: SystemTime,
}

/// What `decide_pending` needs to know about the exe.
#[derive(Debug, Clone)]
pub struct ExeFacts {
    pub image: Image,
    pub modified: SystemTime,
}

/// The decision, from facts only (no file access, no clock of its own).
///
/// `tmp`: None when there is no .tmp, `Some(Err)` when it cannot be used.
/// `companion`: None when there is no companion file. `exe` reads the exe;
/// it is called only when there is a usable .tmp and a readable companion
/// (or none).
///
/// Dates are compared in one case only: a .tmp without companion (left by a
/// version before issue 055, pass 2), which is a conflict when the exe is
/// strictly more recent and holds other notes.
pub fn decide_pending(
    tmp: Option<&Result<StagedFacts, IgnoreReason>>,
    companion: Option<&Result<Companion, CompanionError>>,
    exe: &dyn Fn() -> Result<ExeFacts, String>,
) -> PendingDecision {
    let tmp = match tmp {
        None => return PendingDecision::None,
        Some(Err(IgnoreReason::Unreadable(e))) => return PendingDecision::TmpUnreadable(e.clone()),
        Some(Err(reason)) => return PendingDecision::Ignored(reason.clone()),
        Some(Ok(t)) => t,
    };
    // A companion that cannot be read says nothing (base, program): the
    // .tmp may be the latest save, as when the .tmp itself cannot be read
    // (refuter pass 3, A1, issue 055).
    if let Some(Err(CompanionError::Unreadable(e))) = companion {
        return PendingDecision::TmpUnreadable(e.clone());
    }
    let from_updater = matches!(companion, Some(Ok(c)) if c.program_from_updater);
    // Without notes, only a program staged by the updater is an image.
    if tmp.image.payload.is_none() && !from_updater {
        return PendingDecision::Ignored(IgnoreReason::NoMarker);
    }
    let exe = match exe() {
        Ok(e) => e,
        Err(e) => return PendingDecision::ExeUnreadable(e),
    };
    if tmp.image.payload == exe.image.payload
        && (!from_updater || tmp.image.program == exe.image.program)
    {
        return PendingDecision::Stale;
    }
    let conflict = |cause: ConflictCause| PendingDecision::Conflict {
        tmp_modified: tmp.modified,
        exe_modified: exe.modified,
        exe_has_data: exe.image.payload.is_some(),
        cause,
    };
    match companion {
        Some(Ok(c)) => {
            if c.base == notes_sha(exe.image.payload.as_deref()) {
                PendingDecision::Promote
            } else {
                conflict(ConflictCause::ExeChanged)
            }
        }
        Some(Err(CompanionError::Invalid(e))) => conflict(ConflictCause::CompanionInvalid(e.clone())),
        Some(Err(CompanionError::Unreadable(e))) => PendingDecision::TmpUnreadable(e.clone()),
        None => {
            if exe.modified > tmp.modified {
                conflict(ConflictCause::LegacyExeNewer)
            } else {
                PendingDecision::Promote
            }
        }
    }
}

/// The facts about the .tmp at `tmp_path`, or None when there is no such file.
/// The text of `Unreadable` names the file.
pub fn staged_facts(tmp_path: &Path) -> Option<Result<StagedFacts, IgnoreReason>> {
    let unreadable = |e: String| {
        Some(Err(IgnoreReason::Unreadable(format!(
            "the pending save {} cannot be read: {}",
            tmp_path.display(),
            e
        ))))
    };
    let metadata = match fs::symlink_metadata(tmp_path) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return None,
        Err(e) => return unreadable(e.to_string()),
    };
    if !metadata.file_type().is_file() {
        return unreadable("not a regular file".to_string());
    }
    let modified = match metadata.modified() {
        Ok(m) => m,
        Err(e) => return unreadable(format!("modification time: {}", e)),
    };
    let data = match fs::read(tmp_path) {
        Ok(d) => d,
        Err(e) => return unreadable(e.to_string()),
    };
    Some(staged_image(&data).map(|image| StagedFacts { image, modified }))
}

/// The facts about the exe. The error names the file.
pub fn exe_facts(exe_path: &Path) -> Result<ExeFacts, String> {
    let unreadable = |e: io::Error| format!("the program file {} cannot be read: {}", exe_path.display(), e);
    let modified = fs::metadata(exe_path)
        .and_then(|m| m.modified())
        .map_err(unreadable)?;
    let data = fs::read(exe_path).map_err(unreadable)?;
    Ok(ExeFacts { image: exe_image(&data), modified })
}
