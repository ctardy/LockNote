// Backup — full export / import of a note.
//
// A backup carries everything the note holds except machine state: tabs,
// snapshots, prompts, vault, attachments and settings (TOTP included). It is
// encrypted with the note password, never written in clear.
//
// File layout:
//
//   [MAGIC "LNBACKUP" 8][format u16 LE][crypto wire bytes]
//
//   crypto plaintext = compression::compress(JSON)      (see `crypto`)
//
// The JSON object is built here, not with the marker-based text of the exe
// payload: markers are fragile in free text (issue 050) and a block added by a
// later version would be glued into the tabs by an older reader.
//
// Compatibility contract (never break it, see docs/SPEC.md "Full backup"):
//   * `format` is the major version. A build reads every format up to
//     `FORMAT` and writes `FORMAT`. A higher format is refused with a clear
//     message BEFORE the password is asked (`peek_format`).
//   * Inside a format, additions are compatible: readers ignore unknown JSON
//     fields and give missing optional fields their default. Anything that
//     changes the meaning of an existing field, or removes one, bumps `format`.
//   * The format is repeated inside the authenticated JSON and must match the
//     one in the clear header, so the header cannot be edited to downgrade.
//   * `fixtures/v1_basic.lnbak` is a frozen format-1 file. It must keep
//     decoding forever; never regenerate it. Same for `v1_all_settings.lnbak`
//     and `v1_prompts.lnbak` ("prompts", added inside format 1 by issue 058:
//     absent from older files, read as no prompt).
//
// Not exported on purpose: session state (active tab, caret line, keys loaded
// in ssh-agent) and window geometry. They belong to this machine and session.

use crate::features::attachments::Attachment;
use crate::features::prompts::PromptEntry;
use crate::features::snapshots::Snapshot;
use crate::features::tabs::NoteTab;
use crate::features::vault::SecretEntry;
use crate::settings::Settings;
use base64::Engine;
use serde_json::{Map, Value};
use zeroize::Zeroizing;

/// Clear magic at the start of every backup file.
pub const MAGIC: &[u8; 8] = b"LNBACKUP";
/// Highest format this build reads, and the one it writes.
pub const FORMAT: u16 = 1;
/// Extension proposed by the dialogs (without the dot).
pub const FILE_EXTENSION: &str = "lnbak";
/// Upper bound accepted by the import dialog before reading the file.
pub const MAX_FILE_SIZE: u64 = 512 * 1024 * 1024;
/// Largest JSON body written. `compression::decompress` stops at 256 MiB, so a
/// bigger body could be written but never read back: refuse it at export.
pub const MAX_JSON_BYTES: usize = 200 * 1024 * 1024;
/// Ceilings on the Argon2id cost read from the (unauthenticated) header of a
/// file picked by the user, checked before any key derivation. About 8x the
/// cost used today: a forged header cannot freeze the app for minutes.
pub const MAX_KDF_MEMORY_KIB: u32 = 512 * 1024;
pub const MAX_KDF_ITERATIONS: u32 = 20;
pub const MAX_KDF_LANES: u32 = 16;

const HEADER_LEN: usize = MAGIC.len() + 2;
const KIND: &str = "locknote-full-backup";

/// Argon2id parameters: memory (KiB), iterations, lanes.
pub type Kdf = (u32, u32, u32);

/// The parameters used for real exports (same as saving the note).
pub const DEFAULT_KDF: Kdf = (
    crate::crypto::DEFAULT_M_COST,
    crate::crypto::DEFAULT_T_COST,
    crate::crypto::DEFAULT_P_LANES,
);

/// Everything a backup holds.
#[derive(Debug, Clone)]
pub struct Backup {
    pub settings: Settings,
    /// Always at least one tab after `decode`.
    pub tabs: Vec<NoteTab>,
    pub snapshots: Vec<Snapshot>,
    pub prompts: Vec<PromptEntry>,
    pub secrets: Vec<SecretEntry>,
    pub attachments: Vec<Attachment>,
    /// Unix time of the export (0 when unknown).
    pub created_unix: u64,
    /// LockNote version that wrote the file (empty when unknown).
    pub app_version: String,
}

impl Backup {
    /// Wipe every plaintext this backup owns. Call it on the copy built for an
    /// export once the file is written: `on_save` wipes its buffers too.
    pub fn wipe(&mut self) {
        use zeroize::Zeroize;
        for t in &mut self.tabs {
            t.content.zeroize();
            t.saved_content.zeroize();
        }
        for s in &mut self.snapshots {
            s.wipe();
        }
        for a in &mut self.attachments {
            a.wipe();
        }
        // SecretEntry, PromptEntry and TotpEntry are ZeroizeOnDrop.
        self.secrets.clear();
        self.prompts.clear();
        self.settings.totp_entries.clear();
        self.settings.totp_2fa_secret.zeroize();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackupError {
    /// Not a LockNote backup (bad magic, truncated header, format 0).
    NotABackup,
    /// Written by a newer LockNote: `found` > `supported`.
    NewerFormat { found: u16, supported: u16 },
    /// The clear header and the authenticated JSON disagree on the format.
    FormatMismatch,
    /// HMAC failure: wrong password or altered file (indistinguishable on
    /// purpose, see `CryptoError::HmacMismatch`).
    WrongPasswordOrCorrupt,
    /// Structurally invalid content (not JSON, wrong field type, ...).
    Corrupt(String),
    /// Encryption failed while writing (or the password is empty).
    Crypto(String),
    /// The note is too big for a backup that can be read back.
    TooLarge,
}

impl std::fmt::Display for BackupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotABackup => write!(f, "not a LockNote backup file"),
            Self::NewerFormat { found, supported } => write!(
                f,
                "backup format {} is newer than this LockNote supports (format {})",
                found, supported
            ),
            Self::FormatMismatch => write!(f, "backup header and content disagree on the format"),
            Self::WrongPasswordOrCorrupt => write!(f, "wrong password or corrupted backup"),
            Self::Corrupt(s) => write!(f, "corrupted backup: {}", s),
            Self::Crypto(s) => write!(f, "backup encryption failed: {}", s),
            Self::TooLarge => write!(
                f,
                "the note is too large for a full backup (limit {} MiB of data)",
                MAX_JSON_BYTES / (1024 * 1024)
            ),
        }
    }
}

impl std::error::Error for BackupError {}

/// Check the clear header only: magic and format. Lets the UI refuse a file
/// from a newer version without asking for the password. Returns the format.
pub fn peek_format(bytes: &[u8]) -> Result<u16, BackupError> {
    if bytes.len() < HEADER_LEN || &bytes[..MAGIC.len()] != MAGIC {
        return Err(BackupError::NotABackup);
    }
    let format = u16::from_le_bytes([bytes[MAGIC.len()], bytes[MAGIC.len() + 1]]);
    if format == 0 {
        return Err(BackupError::NotABackup);
    }
    if format > FORMAT {
        return Err(BackupError::NewerFormat { found: format, supported: FORMAT });
    }
    Ok(format)
}

/// Settings as they travel: window geometry, side panel widths and the active
/// tab are machine or session state, so they are reset to the defaults.
fn portable_settings(settings: &Settings) -> Settings {
    let defaults = Settings::default();
    let mut s = settings.clone();
    s.window_width = defaults.window_width;
    s.window_height = defaults.window_height;
    s.window_x = defaults.window_x;
    s.window_y = defaults.window_y;
    s.window_maximized = defaults.window_maximized;
    s.vault_panel_width = defaults.vault_panel_width;
    s.totp_panel_width = defaults.totp_panel_width;
    s.attach_panel_width = defaults.attach_panel_width;
    s.prompts_panel_width = defaults.prompts_panel_width;
    s.active_tab = 0;
    s
}

/// Append `s` as a JSON string literal. Hand-written so secrets go straight
/// into the caller's wiped buffer instead of through temporary `String`s.
fn push_json_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Rough size of the JSON, so the wiped buffer is allocated once: a growing
/// `String` would leave an unwiped copy of the secrets at every reallocation.
fn estimate_json_len(backup: &Backup) -> usize {
    let text: usize = backup.tabs.iter().map(|t| t.name.len() + t.content.len() + 40).sum::<usize>()
        + backup.snapshots.iter().map(|s| s.name.len() + s.content.len() + 60).sum::<usize>()
        + backup.secrets.iter().map(|s| s.name.len() + s.value.len() + s.category.len() + 60).sum::<usize>()
        + backup.prompts.iter().map(|p| p.name.len() + p.text.len() + p.category.len() + 60).sum::<usize>()
        + backup.attachments.iter().map(|a| a.name.len() + a.data.len() / 3 * 4 + 60).sum::<usize>();
    // x2 absorbs JSON escaping of control characters and quotes.
    text.saturating_mul(2).saturating_add(8 * 1024)
}

fn to_json(backup: &Backup, format: u16) -> Zeroizing<String> {
    let mut o: Zeroizing<String> = Zeroizing::new(String::with_capacity(estimate_json_len(backup)));
    o.push_str(&format!("{{\"format\":{},\"kind\":\"{}\",\"created_unix\":{},\"app_version\":", format, KIND, backup.created_unix));
    push_json_str(&mut o, &backup.app_version);

    o.push_str(",\"settings\":");
    // The portable copy holds the 2FA secret: wiped once serialized (its
    // TOTP entries wipe themselves on drop; issue #41).
    let mut portable = portable_settings(&backup.settings);
    let settings_text: Zeroizing<String> = Zeroizing::new(portable.serialize(""));
    zeroize::Zeroize::zeroize(&mut portable.totp_2fa_secret);
    push_json_str(&mut o, &settings_text);

    o.push_str(",\"tabs\":[");
    for (i, t) in backup.tabs.iter().enumerate() {
        if i > 0 {
            o.push(',');
        }
        o.push_str("{\"name\":");
        push_json_str(&mut o, &t.name);
        o.push_str(",\"content\":");
        push_json_str(&mut o, &t.content);
        if let Some(color) = &t.color {
            o.push_str(",\"color\":");
            push_json_str(&mut o, color);
        }
        o.push('}');
    }

    o.push_str("],\"snapshots\":[");
    for (i, s) in backup.snapshots.iter().enumerate() {
        if i > 0 {
            o.push(',');
        }
        o.push_str(&format!("{{\"timestamp\":{},\"name\":", s.timestamp));
        push_json_str(&mut o, &s.name);
        o.push_str(",\"content\":");
        push_json_str(&mut o, &s.content);
        o.push('}');
    }

    o.push_str("],\"prompts\":[");
    for (i, p) in backup.prompts.iter().enumerate() {
        if i > 0 {
            o.push(',');
        }
        o.push_str("{\"name\":");
        push_json_str(&mut o, &p.name);
        o.push_str(",\"text\":");
        push_json_str(&mut o, &p.text);
        o.push_str(",\"category\":");
        push_json_str(&mut o, &p.category);
        o.push('}');
    }

    o.push_str("],\"vault\":[");
    for (i, s) in backup.secrets.iter().enumerate() {
        if i > 0 {
            o.push(',');
        }
        o.push_str("{\"name\":");
        push_json_str(&mut o, &s.name);
        o.push_str(",\"value\":");
        push_json_str(&mut o, &s.value);
        o.push_str(",\"category\":");
        push_json_str(&mut o, &s.category);
        o.push('}');
    }

    o.push_str("],\"attachments\":[");
    for (i, a) in backup.attachments.iter().enumerate() {
        if i > 0 {
            o.push(',');
        }
        o.push_str("{\"name\":");
        push_json_str(&mut o, &a.name);
        o.push_str(",\"data_b64\":\"");
        base64::engine::general_purpose::STANDARD.encode_string(&a.data, &mut o);
        o.push_str("\"}");
    }
    o.push_str("]}");
    o
}

/// Wrap `json` in the container with `format` in the clear header.
fn seal(json: &str, format: u16, password: &str, kdf: Kdf) -> Result<Vec<u8>, BackupError> {
    let compressed: Zeroizing<String> = Zeroizing::new(crate::features::compression::compress(json));
    let encrypted = crate::crypto::encrypt(&compressed, password, kdf.0, kdf.1, kdf.2)
        .map_err(|e| BackupError::Crypto(e.to_string()))?;
    let mut out = Vec::with_capacity(HEADER_LEN + encrypted.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&format.to_le_bytes());
    out.extend_from_slice(&encrypted);
    Ok(out)
}

/// Refuse a JSON body that could be written but not read back.
fn check_json_size(len: usize) -> Result<(), BackupError> {
    if len > MAX_JSON_BYTES {
        return Err(BackupError::TooLarge);
    }
    Ok(())
}

/// Build the backup file bytes. An empty password is refused: it is what a
/// locked note holds, and the result would open for anyone.
pub fn encode(backup: &Backup, password: &str, kdf: Kdf) -> Result<Vec<u8>, BackupError> {
    if password.is_empty() {
        return Err(BackupError::Crypto("empty password".into()));
    }
    let json = to_json(backup, FORMAT);
    check_json_size(json.len())?;
    seal(&json, FORMAT, password, kdf)
}

/// Check the Argon2id cost announced by the file before deriving anything.
/// `wire` is the crypto layer: salt[16] iv[16] m[4] t[4] p[4] ...
fn check_kdf_cost(wire: &[u8]) -> Result<(), BackupError> {
    let at = crate::crypto::SALT_SIZE + crate::crypto::IV_SIZE;
    if wire.len() < at + crate::crypto::ARGON2_PARAMS_SIZE {
        return Ok(()); // too short: `decrypt` reports it
    }
    let word = |i: usize| u32::from_le_bytes([wire[at + i], wire[at + i + 1], wire[at + i + 2], wire[at + i + 3]]);
    let (m, t, p) = (word(0), word(4), word(8));
    if m > MAX_KDF_MEMORY_KIB || t > MAX_KDF_ITERATIONS || p > MAX_KDF_LANES {
        return Err(BackupError::Corrupt("unreasonable key derivation settings".into()));
    }
    Ok(())
}

/// Read a backup file. Every failure is an explicit `BackupError`; nothing is
/// guessed or defaulted when the content is structurally wrong.
pub fn decode(bytes: &[u8], password: &str) -> Result<Backup, BackupError> {
    let format = peek_format(bytes)?;
    check_kdf_cost(&bytes[HEADER_LEN..])?;
    let compressed: Zeroizing<String> = Zeroizing::new(
        crate::crypto::decrypt(&bytes[HEADER_LEN..], password).map_err(|e| match e {
            crate::crypto::CryptoError::HmacMismatch => BackupError::WrongPasswordOrCorrupt,
            other => BackupError::Corrupt(other.to_string()),
        })?,
    );
    let json: Zeroizing<String> = Zeroizing::new(crate::features::compression::decompress(&compressed));
    let root: Value = serde_json::from_str(&json)
        .map_err(|_| BackupError::Corrupt("content is not valid JSON".into()))?;
    drop(json);

    let Value::Object(mut root) = root else {
        return Err(BackupError::Corrupt("content is not a JSON object".into()));
    };
    let inner = root.get("format").and_then(Value::as_u64);
    if inner != Some(u64::from(format)) {
        return Err(BackupError::FormatMismatch);
    }

    match format {
        1 => parse_v1(&mut root),
        // peek_format only lets 1..=FORMAT through, so a new FORMAT must add
        // its parser here; failing loudly beats silently reading it as v1.
        other => Err(BackupError::Corrupt(format!("no reader for format {}", other))),
    }
}

/// Read the data of another LockNote file (an older exe, or its staging file):
/// `raw_payload` is what `storage::read_data` returns (everything after the
/// marker). The result is the same `Backup` an import applies, so migrating
/// from an old exe and restoring a `.lnbak` share one confirmation and one
/// apply path. Session state (active tab, caret, ssh-agent keys) is dropped,
/// like in an export.
pub fn decode_exe_payload(raw_payload: &[u8], password: &str) -> Result<Backup, BackupError> {
    let (_hint, encrypted) = crate::storage::split_hint_and_encrypted(raw_payload);
    check_kdf_cost(&encrypted)?;
    let plain: Zeroizing<String> = Zeroizing::new(
        crate::crypto::decrypt(&encrypted, password).map_err(|e| match e {
            crate::crypto::CryptoError::HmacMismatch => BackupError::WrongPasswordOrCorrupt,
            other => BackupError::Corrupt(other.to_string()),
        })?,
    );
    let text: Zeroizing<String> = Zeroizing::new(crate::features::compression::decompress(&plain));
    drop(plain);
    let (mut settings, note_text) = Settings::parse(&text);
    let note_text: Zeroizing<String> = Zeroizing::new(note_text);
    drop(text);

    let categories = crate::features::vault::categories::effective(&settings.vault_categories);
    let parsed = crate::features::payload::parse_payload(&note_text, &categories);
    let portable = portable_settings(&settings);
    // The settings read hold the 2FA secret: wiped, the backup keeps its
    // own copy (the TOTP entries wipe themselves on drop; issue #41).
    zeroize::Zeroize::zeroize(&mut settings.totp_2fa_secret);
    Ok(Backup {
        settings: portable,
        tabs: parsed.tabs,
        snapshots: parsed.snapshots,
        prompts: parsed.prompts,
        secrets: parsed.secrets,
        attachments: parsed.attachments,
        created_unix: 0,
        app_version: String::new(),
    })
}

fn take_string(m: &mut Map<String, Value>, key: &str) -> Result<String, BackupError> {
    match m.remove(key) {
        Some(Value::String(s)) => Ok(s),
        _ => Err(BackupError::Corrupt(format!("missing or invalid \"{}\"", key))),
    }
}

fn take_opt_string(m: &mut Map<String, Value>, key: &str) -> Result<Option<String>, BackupError> {
    match m.remove(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(_) => Err(BackupError::Corrupt(format!("invalid \"{}\"", key))),
    }
}

fn take_array(m: &mut Map<String, Value>, key: &str) -> Result<Vec<Value>, BackupError> {
    match m.remove(key) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(a)) => Ok(a),
        Some(_) => Err(BackupError::Corrupt(format!("invalid \"{}\"", key))),
    }
}

fn into_object(v: Value, what: &str) -> Result<Map<String, Value>, BackupError> {
    match v {
        Value::Object(m) => Ok(m),
        _ => Err(BackupError::Corrupt(format!("invalid {} entry", what))),
    }
}

/// Format 1. Unknown fields are ignored (forward compatibility); optional
/// blocks that are absent are empty.
fn parse_v1(root: &mut Map<String, Value>) -> Result<Backup, BackupError> {
    let settings = match take_opt_string(root, "settings")? {
        Some(text) => {
            // The settings text holds the secrets: wiped once read (issue #41).
            let text = Zeroizing::new(text);
            Settings::parse(&text).0
        }
        None => Settings::default(),
    };

    let mut tabs = Vec::new();
    for v in take_array(root, "tabs")? {
        let mut m = into_object(v, "tab")?;
        let name = take_string(&mut m, "name")?;
        let content = take_string(&mut m, "content")?;
        let color = take_opt_string(&mut m, "color")?;
        tabs.push(NoteTab::with_content(&name, &content, color.as_deref()));
    }
    if tabs.is_empty() {
        // Same contract as `tabs::parse`: there is always at least one tab.
        tabs.push(NoteTab::with_content("Note 1", "", None));
    }

    let mut snapshots = Vec::new();
    for v in take_array(root, "snapshots")? {
        let mut m = into_object(v, "snapshot")?;
        let timestamp = m
            .remove("timestamp")
            .and_then(|t| t.as_u64())
            .ok_or_else(|| BackupError::Corrupt("missing or invalid \"timestamp\"".into()))?;
        let name = take_opt_string(&mut m, "name")?.unwrap_or_default();
        let content = take_string(&mut m, "content")?;
        snapshots.push(Snapshot { timestamp, name, content });
    }

    // Added inside format 1 (issue 058): absent from older files, no prompt.
    // A prompt the note could not hold (see `prompts::check_entry`) is
    // refused, not changed.
    let mut prompts = Vec::new();
    for v in take_array(root, "prompts")? {
        let mut m = into_object(v, "prompt")?;
        let name = take_string(&mut m, "name")?;
        let text = take_string(&mut m, "text")?;
        let category = take_string(&mut m, "category")?;
        crate::features::prompts::check_entry(&name, &text, &category)
            .map_err(|e| BackupError::Corrupt(format!("prompt {}: {}", prompts.len() + 1, e)))?;
        prompts.push(PromptEntry { name, text, category });
    }

    let mut secrets = Vec::new();
    for v in take_array(root, "vault")? {
        let mut m = into_object(v, "vault")?;
        let name = take_string(&mut m, "name")?;
        let value = take_string(&mut m, "value")?;
        let category = take_opt_string(&mut m, "category")?.unwrap_or_else(|| "General".to_string());
        // `in_category`, not `new`: the backup's categories were valid in the note
        // it came from, and custom ones (issue 052) must not fall back to General.
        secrets.push(SecretEntry::in_category(&name, &value, &category));
    }

    let mut attachments = Vec::new();
    for v in take_array(root, "attachments")? {
        let mut m = into_object(v, "attachment")?;
        let name = take_string(&mut m, "name")?;
        let b64 = take_string(&mut m, "data_b64")?;
        let data = base64::engine::general_purpose::STANDARD
            .decode(b64.as_bytes())
            .map_err(|_| BackupError::Corrupt(format!("attachment \"{}\" is not valid base64", name)))?;
        attachments.push(Attachment::new(&name, data));
    }

    Ok(Backup {
        settings,
        tabs,
        snapshots,
        prompts,
        secrets,
        attachments,
        created_unix: match root.remove("created_unix") {
            None | Some(Value::Null) => 0,
            Some(v) => v
                .as_u64()
                .ok_or_else(|| BackupError::Corrupt("invalid \"created_unix\"".into()))?,
        },
        app_version: take_opt_string(root, "app_version")?.unwrap_or_default(),
    })
}

#[cfg(test)]
mod json_tool;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::prompts::PromptEntry;

    const PW: &str = "correct horse";
    /// Smallest valid Argon2 parameters: keeps the tests fast.
    const FAST: Kdf = (8, 1, 1);

    fn sample() -> Backup {
        let mut settings = Settings::default();
        settings.theme = crate::settings::ThemeChoice::Light;
        settings.auto_lock = 7;
        settings.vault_categories = vec!["Work".into(), "Home".into()];
        settings.totp_2fa = true;
        settings.totp_2fa_secret = "JBSWY3DPEHPK3PXP".into();
        Backup {
            settings,
            tabs: vec![
                NoteTab::with_content("Alpha", "first\nsecond", None),
                NoteTab::with_content("Beta", "b", Some("blue")),
            ],
            snapshots: vec![Snapshot::with_name(1_700_000_000, "v1", "old text")],
            prompts: vec![
                PromptEntry::new("Summarize", "Summarize this:\n\n", "Writing"),
                PromptEntry::new("Review", "Review the diff", "Code"),
            ],
            secrets: vec![SecretEntry::new("gmail", "s3cret", "Email")],
            attachments: vec![Attachment::new("a.bin", vec![0, 1, 2, 255, 254])],
            created_unix: 1_760_000_000,
            app_version: "1.2.17".into(),
        }
    }

    fn tab_view(tabs: &[NoteTab]) -> Vec<(String, String, Option<String>)> {
        tabs.iter().map(|t| (t.name.clone(), t.content.clone(), t.color.clone())).collect()
    }

    /// Wrap hand-written JSON like a future/odd writer would.
    fn seal_json(json: &str, outer_format: u16) -> Vec<u8> {
        seal(json, outer_format, PW, FAST).unwrap()
    }

    /// The bytes after the marker of an exe as the versions with snapshots
    /// wrote them: always the tabs block, then the history block that
    /// `on_save` no longer writes, vault, attachments, session, settings
    /// header, compression, encryption, with an optional plaintext hint
    /// prefix. For what `on_save` writes today, see
    /// `exe_payload_with_reserved_markers_migrates_whole` (`payload::compose`).
    fn exe_payload(b: &Backup, hint: Option<&str>, compress: bool) -> Vec<u8> {
        let mut text = crate::features::tabs::serialize(&b.tabs);
        text = crate::features::snapshots::combine(&text, &b.snapshots);
        text = crate::features::vault::combine(&text, &b.secrets);
        text = crate::features::attachments::combine(&text, &b.attachments);
        let session = crate::features::session_state::SessionState {
            active_tab: 1,
            cursor_line: 9,
            agent_keys: vec!["ssh-ed25519 AAAA machine-key".into()],
        };
        text = crate::features::session_state::combine(&text, &session);
        let mut plain = b.settings.serialize(&text);
        if compress {
            plain = crate::features::compression::compress(&plain);
        }
        let enc = crate::crypto::encrypt(&plain, PW, FAST.0, FAST.1, FAST.2).unwrap();
        let mut out = Vec::new();
        if let Some(h) = hint {
            out.extend_from_slice(b"HNT\x00");
            out.extend_from_slice(&(h.len() as u16).to_le_bytes());
            out.extend_from_slice(h.as_bytes());
        }
        out.extend_from_slice(&enc);
        out
    }

    #[test]
    fn exe_payload_migrates_every_block() {
        // Categories must belong to the user's list, as in a real note.
        let mut b = sample();
        b.secrets = vec![
            SecretEntry::in_category("gmail", "s3cret", "Work"),
            SecretEntry::in_category("nas", "p@ss", "Home"),
        ];
        for (hint, compress) in [(None, true), (Some("my hint"), true), (None, false)] {
            let got = decode_exe_payload(&exe_payload(&b, hint, compress), PW).unwrap();
            assert_eq!(tab_view(&got.tabs), tab_view(&b.tabs));
            assert_eq!(got.snapshots, b.snapshots);
            // That layout has no prompts block (versions before issue 058).
            assert!(got.prompts.is_empty());
            assert_eq!(got.secrets, b.secrets);
            assert_eq!(got.attachments, b.attachments);
            assert_eq!(got.settings.vault_categories, b.settings.vault_categories);
            assert_eq!(got.settings.auto_lock, 7);
            assert!(got.settings.totp_2fa);
            assert_eq!(got.settings.totp_2fa_secret, "JBSWY3DPEHPK3PXP");
        }
    }

    #[test]
    fn exe_payload_drops_machine_and_session_state() {
        let mut b = sample();
        b.settings.window_x = 321;
        b.settings.window_width = 777;
        b.settings.window_maximized = true;
        b.settings.active_tab = 1;
        b.settings.vault_panel_width = 501;
        b.settings.totp_panel_width = 502;
        b.settings.attach_panel_width = 503;
        b.settings.prompts_panel_width = 504;
        let got = decode_exe_payload(&exe_payload(&b, None, true), PW).unwrap();
        let d = Settings::default();
        assert_eq!(got.settings.window_x, d.window_x);
        assert_eq!(got.settings.window_width, d.window_width);
        assert_eq!(got.settings.window_maximized, d.window_maximized);
        assert_eq!(got.settings.active_tab, d.active_tab);
        assert_eq!(got.settings.vault_panel_width, d.vault_panel_width);
        assert_eq!(got.settings.totp_panel_width, d.totp_panel_width);
        assert_eq!(got.settings.attach_panel_width, d.attach_panel_width);
        assert_eq!(got.settings.prompts_panel_width, d.prompts_panel_width);
    }

    #[test]
    fn exe_payload_wrong_password_and_garbage() {
        let payload = exe_payload(&sample(), None, true);
        assert_eq!(decode_exe_payload(&payload, "nope").unwrap_err(), BackupError::WrongPasswordOrCorrupt);
        let mut flipped = payload.clone();
        let last = flipped.len() - 1;
        flipped[last] ^= 1;
        assert_eq!(decode_exe_payload(&flipped, PW).unwrap_err(), BackupError::WrongPasswordOrCorrupt);
        assert!(decode_exe_payload(&[7u8; 10], PW).is_err());
        assert!(decode_exe_payload(&[], PW).is_err());
    }

    #[test]
    fn exe_payload_refuses_forged_kdf_cost() {
        let mut payload = exe_payload(&sample(), None, true);
        let at = crate::crypto::SALT_SIZE + crate::crypto::IV_SIZE;
        payload[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(decode_exe_payload(&payload, PW), Err(BackupError::Corrupt(_))));
    }

    /// The whole chain on a real file layout: program + marker + payload, read
    /// back with the production `storage` functions.
    #[test]
    fn exe_file_layout_roundtrip() {
        let payload = exe_payload(&sample(), Some("h"), true);
        let mut file = vec![0x4D, 0x5A, 0x90, 0x00];
        file.extend(std::iter::repeat(0x11).take(500));
        file.extend_from_slice(&crate::storage::get_marker_for_update());
        file.extend_from_slice(&payload);
        let pos = crate::storage::find_marker(&file).unwrap();
        let after = &file[pos + 16..];
        let got = decode_exe_payload(after, PW).unwrap();
        assert_eq!(got.tabs.len(), 2);
    }

    #[test]
    fn roundtrip_all_blocks() {
        let b = sample();
        let bytes = encode(&b, PW, FAST).unwrap();
        let back = decode(&bytes, PW).unwrap();

        assert_eq!(tab_view(&back.tabs), tab_view(&b.tabs));
        assert_eq!(back.snapshots, b.snapshots);
        assert_eq!(back.prompts, b.prompts);
        assert_eq!(back.secrets, b.secrets);
        assert_eq!(back.attachments, b.attachments);
        assert_eq!(back.created_unix, 1_760_000_000);
        assert_eq!(back.app_version, "1.2.17");
        assert_eq!(back.settings.theme, b.settings.theme);
        assert_eq!(back.settings.auto_lock, 7);
        assert_eq!(back.settings.vault_categories, b.settings.vault_categories);
        assert!(back.settings.totp_2fa);
        assert_eq!(back.settings.totp_2fa_secret, "JBSWY3DPEHPK3PXP");
    }

    #[test]
    fn custom_vault_categories_survive() {
        let mut b = sample();
        b.settings.vault_categories = vec!["Work".into(), "Home".into()];
        b.secrets = vec![
            SecretEntry::in_category("a", "1", "Work"),
            SecretEntry::in_category("b", "2", "Home"),
        ];
        let back = decode(&encode(&b, PW, FAST).unwrap(), PW).unwrap();
        assert_eq!(back.secrets, b.secrets);
        assert_eq!(back.settings.vault_categories, vec!["Work".to_string(), "Home".to_string()]);
    }

    /// Every `Settings` field is listed below, on purpose and without `..`.
    /// Adding a field to `Settings` stops this test from compiling until
    /// someone decides whether it travels in a backup (add it to the
    /// round-trip below, and to the docs) or is machine state (reset it in
    /// `portable_settings` and in `machine_state_is_not_exported`).
    #[test]
    fn every_settings_field_has_a_decision() {
        let Settings {
            // travels (checked by `settings_roundtrip_every_field`)
            save_on_close: _,
            theme: _,
            word_wrap: _,
            minimize_to_tray: _,
            min_password_length: _,
            font_family: _,
            font_size: _,
            save_key: _,
            save_modifiers: _,
            language: _,
            auto_lock: _,
            clipboard_clear: _,
            auto_save: _,
            show_line_numbers: _,
            totp_2fa: _,
            totp_2fa_secret: _,
            totp_entries: _,
            vault_categories: _,
            hotkey_modifiers: _,
            hotkey_key: _,
            max_attachment_size_mb: _,
            max_total_attachments_mb: _,
            passgen: _,
            // machine / session state, reset by `portable_settings`
            window_width: _,
            window_height: _,
            window_x: _,
            window_y: _,
            window_maximized: _,
            active_tab: _,
            vault_panel_width: _,
            totp_panel_width: _,
            attach_panel_width: _,
            prompts_panel_width: _,
        } = Settings::default();
    }

    /// Every field set to a value that differs from the default, machine state
    /// included (it must not travel).
    fn full_settings() -> Settings {
        use crate::settings::{CloseAction, Language, ThemeChoice, TotpEntry};
        let mut s = Settings::default();
        s.save_on_close = CloseAction::Always;
        s.theme = ThemeChoice::Light;
        s.word_wrap = !s.word_wrap;
        s.minimize_to_tray = !s.minimize_to_tray;
        s.min_password_length = 11;
        s.font_family = "Cascadia Mono".into();
        s.font_size = 13.5;
        s.save_key = 0x44;
        s.save_modifiers = 0x06;
        s.language = Language::De;
        s.auto_lock = 3;
        s.clipboard_clear = 12;
        s.auto_save = 45;
        s.show_line_numbers = !s.show_line_numbers;
        s.totp_2fa = true;
        s.totp_2fa_secret = "JBSWY3DPEHPK3PXP".into();
        s.totp_entries = vec![
            TotpEntry { name: "github".into(), secret: "GEZDGNBVGY3TQOJQ".into() },
            TotpEntry { name: "aws".into(), secret: "MFRGGZDFMZTWQ2LK".into() },
        ];
        s.vault_categories = vec!["Work".into(), "Home".into()];
        s.hotkey_modifiers = 3;
        s.hotkey_key = 77;
        s.max_attachment_size_mb = 7;
        s.max_total_attachments_mb = 33;
        s.passgen = crate::features::passgen::Options {
            length: 29,
            upper: false,
            lower: true,
            digits: false,
            symbols: true,
            exclude_ambiguous: true,
            symbol_set: "-_#".into(),
        };
        s.window_width = 1234;
        s.window_height = 777;
        s.window_x = 10;
        s.window_y = 20;
        s.window_maximized = true;
        s.active_tab = 4;
        s.vault_panel_width = 410;
        s.totp_panel_width = 420;
        s.attach_panel_width = 430;
        s.prompts_panel_width = 440;
        s
    }

    #[test]
    fn settings_roundtrip_every_field() {
        let s = full_settings();
        let mut b = sample();
        b.settings = s.clone();
        let back = decode(&encode(&b, PW, FAST).unwrap(), PW).unwrap();

        // Everything that travels is identical; machine state is reset.
        assert_eq!(back.settings, portable_settings(&s));
        assert_eq!(back.settings.totp_entries, s.totp_entries);
        assert_ne!(back.settings, s, "machine state must not travel");
    }

    #[test]
    fn empty_password_is_refused_at_export() {
        assert_eq!(
            encode(&sample(), "", FAST).unwrap_err(),
            BackupError::Crypto("empty password".into())
        );
    }

    #[test]
    fn oversized_body_is_refused_at_export() {
        assert_eq!(check_json_size(MAX_JSON_BYTES), Ok(()));
        assert_eq!(check_json_size(MAX_JSON_BYTES + 1), Err(BackupError::TooLarge));
    }

    #[test]
    fn forged_kdf_cost_is_refused_before_any_derivation() {
        let good = encode(&sample(), PW, FAST).unwrap();
        let at = HEADER_LEN + crate::crypto::SALT_SIZE + crate::crypto::IV_SIZE;
        for (offset, value) in [
            (0usize, MAX_KDF_MEMORY_KIB + 1),
            (0, 1_048_576),
            (4, MAX_KDF_ITERATIONS + 1),
            (8, MAX_KDF_LANES + 1),
        ] {
            let mut forged = good.clone();
            forged[at + offset..at + offset + 4].copy_from_slice(&value.to_le_bytes());
            let started = std::time::Instant::now();
            assert!(matches!(decode(&forged, PW), Err(BackupError::Corrupt(_))), "offset {}", offset);
            assert!(started.elapsed().as_millis() < 500, "must fail without deriving a key");
        }
    }

    #[test]
    fn export_copy_can_be_wiped() {
        let mut b = sample();
        b.wipe();
        assert!(b.tabs.iter().all(|t| t.content.is_empty() && t.saved_content.is_empty()));
        assert!(b.snapshots.iter().all(|s| s.content.is_empty()));
        assert!(b.attachments.iter().all(|a| a.data.iter().all(|&x| x == 0)));
        assert!(b.secrets.is_empty());
        assert!(b.prompts.is_empty());
        assert!(b.settings.totp_entries.is_empty());
        assert!(b.settings.totp_2fa_secret.is_empty());
    }

    #[test]
    fn json_buffer_does_not_reallocate() {
        // A reallocation would leave an unwiped copy of the secrets behind.
        let mut b = sample();
        b.tabs[0].content = "x\"\n\t\u{1}".repeat(5000);
        b.prompts = vec![PromptEntry::new("p\"\u{2}", &"\"\\\n\u{3}".repeat(5000), "c\t")];
        b.attachments = vec![Attachment::new("big.bin", vec![7u8; 100_000])];
        let json = to_json(&b, FORMAT);
        assert!(
            json.len() <= estimate_json_len(&b),
            "estimate {} < actual {}",
            estimate_json_len(&b),
            json.len()
        );
    }

    #[test]
    fn hostile_content_survives() {
        let nasty = "a [TAB:x] b\n[LOCKNOTE_VAULT]\n[/LOCKNOTE_TABS]\0\u{1}\"\\ \t\r\n é 漢 😀 [LOCKNOTE_STATE]\n\
                     [LOCKNOTE_ATTACHMENTS]\r\n[/LOCKNOTE_ATTACHMENTS]\n[LOCKNOTE_HISTORY]\n[/LOCKNOTE_HISTORY]\n\
                     [LOCKNOTE_STATE]\nactive=9\n[/LOCKNOTE_STATE]\n[/LOCKNOTE_VAULT]\n[SECRET:x]\n[/SECRET]\n[FILE:x]\n[/FILE]\n[SNAP:1]\n[/SNAP]";
        let mut b = sample();
        b.tabs = vec![NoteTab::with_content(nasty, nasty, Some("red"))];
        b.snapshots = vec![Snapshot::with_name(1, nasty, nasty)];
        b.secrets = vec![SecretEntry::new(nasty, "line1\nline2\r\n\"q\"", nasty)];
        // A prompt name and category cannot hold a line break (nor the
        // category a `:`), the text can hold anything.
        b.prompts = vec![
            PromptEntry::new("a [TAB:x] [LOCKNOTE_PROMPTS] \"\\ é 漢 😀 \u{1}", nasty, "[/PROMPT] \"q\" \\"),
            PromptEntry::new("[/LOCKNOTE_PROMPTS]", "[/PROMPT]\n[PROMPT:x:y]\n\0\r\n", "[LOCKNOTE_VAULT]"),
        ];
        b.attachments = vec![Attachment::new(nasty, vec![0u8; 3000])];
        // The generator's symbol list may hold every ASCII punctuation
        // character: quotes, backslash, brackets, `=`.
        let all_symbols: String = (0x21u8..0x7F).map(char::from).filter(|c| c.is_ascii_punctuation()).collect();
        b.settings.passgen.symbol_set = all_symbols.clone();
        let back = decode(&encode(&b, PW, FAST).unwrap(), PW).unwrap();
        assert_eq!(tab_view(&back.tabs), tab_view(&b.tabs));
        assert_eq!(back.snapshots, b.snapshots);
        assert_eq!(back.prompts, b.prompts);
        assert_eq!(back.secrets, b.secrets);
        assert_eq!(back.attachments, b.attachments);
        assert_eq!(back.settings.passgen.symbol_set, all_symbols);
    }

    /// Text with every reserved marker of the exe payload at the start of a
    /// line (LF and CRLF), inside a line, and as a line prefix.
    fn text_with_every_marker() -> String {
        let mut text = String::from("first line\r\n");
        for m in crate::features::markers::RESERVED {
            text.push_str(&format!("{}\n{}\r\n  {}\nvoir {} ici\n{}suite\n", m, m, m, m, m));
        }
        text.push_str("last line");
        text
    }

    /// Issue 050 did not touch the backup: tabs and values travel as JSON
    /// strings, so the markers of the exe payload are plain text here and the
    /// `\x00` escape of that payload never reaches the file. Same `FORMAT`.
    #[test]
    fn reserved_markers_travel_as_plain_json_text() {
        assert_eq!(FORMAT, 1);
        let text = text_with_every_marker();
        let mut b = sample();
        b.tabs = vec![
            NoteTab::with_content("Note 1", &text, None),
            NoteTab::with_content("Other", &text, Some("red")),
        ];
        b.snapshots = vec![Snapshot::with_name(1, "s", &text)];
        b.secrets = vec![SecretEntry::new("k", &text, "Email"), SecretEntry::new("after", "kept", "Cloud")];
        b.prompts = vec![PromptEntry::new("k", &text, "Cat"), PromptEntry::new("after", "kept", "Cat")];

        let json = to_json(&b, FORMAT);
        let mut as_json = String::new();
        push_json_str(&mut as_json, &text);
        assert_eq!(json.matches(&format!("\"content\":{}", as_json)).count(), 3, "two tabs and one snapshot");
        assert_eq!(json.matches(&format!("\"value\":{}", as_json)).count(), 1);
        assert_eq!(json.matches(&format!("\"text\":{}", as_json)).count(), 1);
        assert!(!json.contains("\\u0000"), "no payload escape in a backup");

        let back = decode(&encode(&b, PW, FAST).unwrap(), PW).unwrap();
        assert_eq!(tab_view(&back.tabs), tab_view(&b.tabs));
        assert_eq!(back.snapshots, b.snapshots);
        assert_eq!(back.prompts, b.prompts);
        assert_eq!(back.secrets, b.secrets);
        assert_eq!(back.attachments, b.attachments);
    }

    /// "Import from another LockNote" reads the exe payload: text holding the
    /// reserved markers comes back whole from a file saved by this version.
    #[test]
    fn exe_payload_with_reserved_markers_migrates_whole() {
        let text = text_with_every_marker();
        let secrets = vec![
            SecretEntry::in_category("k", &text, "Work"),
            SecretEntry::in_category("after", "kept", "Home"),
        ];
        let attachments = vec![Attachment::new("a.bin", vec![0, 1, 2, 255, 254])];
        let prompts = vec![PromptEntry::new("k", &text, "Cat"), PromptEntry::new("host:port", "kept", "Other")];
        let settings = sample().settings;
        for tabs in [
            vec![NoteTab::with_content("Note 1", &text, None)],
            vec![NoteTab::with_content("A", &text, None), NoteTab::with_content("B", "second", Some("blue"))],
        ] {
            let state = crate::features::session_state::SessionState { active_tab: 0, cursor_line: 3, ..Default::default() };
            let note = crate::features::payload::compose(&tabs, &prompts, &secrets, &attachments, &state);
            let plain = crate::features::compression::compress(&settings.serialize(&note));
            let payload = crate::crypto::encrypt(&plain, PW, FAST.0, FAST.1, FAST.2).unwrap();

            let got = decode_exe_payload(&payload, PW).unwrap();
            assert_eq!(tab_view(&got.tabs), tab_view(&tabs));
            assert_eq!(got.prompts, prompts);
            assert_eq!(got.secrets, secrets);
            assert_eq!(got.attachments, attachments);
            assert!(got.snapshots.is_empty());
        }
    }

    #[test]
    fn empty_backup_has_one_default_tab() {
        let mut b = sample();
        b.tabs.clear();
        b.snapshots.clear();
        b.secrets.clear();
        b.prompts.clear();
        b.attachments.clear();
        let back = decode(&encode(&b, PW, FAST).unwrap(), PW).unwrap();
        assert_eq!(tab_view(&back.tabs), vec![("Note 1".into(), String::new(), None)]);
        assert!(back.snapshots.is_empty() && back.secrets.is_empty() && back.attachments.is_empty());
        assert!(back.prompts.is_empty());
    }

    #[test]
    fn machine_state_is_not_exported() {
        let mut b = sample();
        b.settings.window_x = 123;
        b.settings.window_y = 456;
        b.settings.window_width = 1111;
        b.settings.window_height = 777;
        b.settings.window_maximized = true;
        b.settings.active_tab = 5;
        b.settings.vault_panel_width = 611;
        b.settings.totp_panel_width = 622;
        b.settings.attach_panel_width = 633;
        b.settings.prompts_panel_width = 644;
        let back = decode(&encode(&b, PW, FAST).unwrap(), PW).unwrap();
        let d = Settings::default();
        assert_eq!(back.settings.vault_panel_width, d.vault_panel_width);
        assert_eq!(back.settings.totp_panel_width, d.totp_panel_width);
        assert_eq!(back.settings.attach_panel_width, d.attach_panel_width);
        assert_eq!(back.settings.prompts_panel_width, d.prompts_panel_width);
        assert_eq!(back.settings.window_x, d.window_x);
        assert_eq!(back.settings.window_y, d.window_y);
        assert_eq!(back.settings.window_width, d.window_width);
        assert_eq!(back.settings.window_height, d.window_height);
        assert!(!back.settings.window_maximized);
        assert_eq!(back.settings.active_tab, 0);
    }

    #[test]
    fn file_is_encrypted() {
        let bytes = encode(&sample(), PW, FAST).unwrap();
        let hay = String::from_utf8_lossy(&bytes);
        for secret in ["s3cret", "gmail", "JBSWY3DPEHPK3PXP", "first", "Summarize this", "Review the diff"] {
            assert!(!hay.contains(secret), "{} leaked in clear", secret);
        }
    }

    #[test]
    fn wrong_password_is_refused() {
        let bytes = encode(&sample(), PW, FAST).unwrap();
        assert_eq!(decode(&bytes, "nope").unwrap_err(), BackupError::WrongPasswordOrCorrupt);
    }

    #[test]
    fn tampered_file_is_refused() {
        let mut bytes = encode(&sample(), PW, FAST).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0x01;
        assert_eq!(decode(&bytes, PW).unwrap_err(), BackupError::WrongPasswordOrCorrupt);
    }

    #[test]
    fn not_a_backup() {
        assert_eq!(decode(b"", PW).unwrap_err(), BackupError::NotABackup);
        assert_eq!(decode(b"LNBACKU", PW).unwrap_err(), BackupError::NotABackup);
        assert_eq!(decode(b"PK\x03\x04 some zip bytes here", PW).unwrap_err(), BackupError::NotABackup);
        // Format 0 does not exist.
        let mut zero = b"LNBACKUP".to_vec();
        zero.extend_from_slice(&0u16.to_le_bytes());
        zero.extend_from_slice(&[0u8; 100]);
        assert_eq!(decode(&zero, PW).unwrap_err(), BackupError::NotABackup);
    }

    #[test]
    fn header_only_is_corrupt_not_panic() {
        let mut bytes = b"LNBACKUP".to_vec();
        bytes.extend_from_slice(&FORMAT.to_le_bytes());
        assert!(matches!(decode(&bytes, PW), Err(BackupError::Corrupt(_))));
    }

    #[test]
    fn newer_format_is_refused_before_the_password() {
        let bytes = seal_json("{\"format\":2}", FORMAT + 1);
        assert_eq!(
            peek_format(&bytes).unwrap_err(),
            BackupError::NewerFormat { found: FORMAT + 1, supported: FORMAT }
        );
        // Same answer whatever the password: it is never needed.
        assert_eq!(
            decode(&bytes, "irrelevant").unwrap_err(),
            BackupError::NewerFormat { found: FORMAT + 1, supported: FORMAT }
        );
    }

    #[test]
    fn header_cannot_be_edited_to_downgrade() {
        // A format-2 body with its header rewritten to 1 must be refused.
        let bytes = seal_json("{\"format\":2,\"tabs\":[]}", 1);
        assert_eq!(decode(&bytes, PW).unwrap_err(), BackupError::FormatMismatch);
        // And the other way round.
        let bytes = seal_json("{\"format\":1}", 1);
        let mut edited = bytes.clone();
        edited[MAGIC.len()] = 2; // header now says 2, body says 1
        assert!(decode(&edited, PW).is_err());
    }

    #[test]
    fn missing_format_inside_is_refused() {
        let bytes = seal_json("{\"tabs\":[]}", 1);
        assert_eq!(decode(&bytes, PW).unwrap_err(), BackupError::FormatMismatch);
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let json = r#"{
            "format": 1,
            "kind": "locknote-full-backup",
            "future_block": {"anything": [1, 2, 3]},
            "tabs": [{"name": "T", "content": "c", "color": "green", "pinned": true}],
            "vault": [{"name": "n", "value": "v", "category": "Email", "icon": "key"}],
            "snapshots": [{"timestamp": 5, "name": "s", "content": "x", "author": "me"}],
            "attachments": [{"name": "f", "data_b64": "AAEC", "mime": "x/y"}],
            "prompts": [{"name": "p", "text": "t", "category": "c", "favorite": true}]
        }"#;
        let back = decode(&seal_json(json, 1), PW).unwrap();
        assert_eq!(back.prompts, vec![PromptEntry::new("p", "t", "c")]);
        assert_eq!(tab_view(&back.tabs), vec![("T".into(), "c".into(), Some("green".into()))]);
        assert_eq!(back.secrets, vec![SecretEntry::new("n", "v", "Email")]);
        assert_eq!(back.snapshots, vec![Snapshot::with_name(5, "s", "x")]);
        assert_eq!(back.attachments, vec![Attachment::new("f", vec![0, 1, 2])]);
    }

    #[test]
    fn missing_optional_fields_take_defaults() {
        let back = decode(&seal_json("{\"format\":1}", 1), PW).unwrap();
        assert_eq!(back.settings, Settings::default());
        assert_eq!(back.tabs.len(), 1);
        assert_eq!(back.tabs[0].name, "Note 1");
        assert!(back.snapshots.is_empty() && back.secrets.is_empty() && back.attachments.is_empty());
        // Written before issue 058: no "prompts" field, no prompt.
        assert!(back.prompts.is_empty());
        assert_eq!(back.created_unix, 0);
        assert_eq!(back.app_version, "");

        // A secret without a category lands in General, a snapshot without a name has none.
        let json = r#"{"format":1,"vault":[{"name":"n","value":"v"}],"snapshots":[{"timestamp":1,"content":"c"}]}"#;
        let back = decode(&seal_json(json, 1), PW).unwrap();
        assert_eq!(back.secrets[0].category, "General");
        assert_eq!(back.snapshots[0].name, "");
    }

    #[test]
    fn wrong_types_are_corrupt_not_defaulted() {
        for json in [
            r#"{"format":1,"tabs":"nope"}"#,
            r#"{"format":1,"created_unix":"yesterday"}"#,
            r#"{"format":1,"app_version":7}"#,
            r#"{"format":1,"tabs":[{"content":"no name"}]}"#,
            r#"{"format":1,"tabs":[42]}"#,
            r#"{"format":1,"vault":[{"name":"n"}]}"#,
            r#"{"format":1,"snapshots":[{"timestamp":"x","content":"c"}]}"#,
            r#"{"format":1,"attachments":[{"name":"f","data_b64":"!!not base64!!"}]}"#,
            r#"{"format":1,"prompts":"nope"}"#,
            r#"{"format":1,"prompts":[42]}"#,
            r#"{"format":1,"prompts":[{"text":"t","category":"c"}]}"#,
            r#"{"format":1,"prompts":[{"name":"n","category":"c"}]}"#,
            r#"{"format":1,"prompts":[{"name":"n","text":"t"}]}"#,
            r#"{"format":1,"prompts":[{"name":"n","text":7,"category":"c"}]}"#,
            r#"{"format":1,"prompts":[{"name":"n","text":"t","category":null}]}"#,
            r#"[1,2,3]"#,
            r#"not json at all"#,
        ] {
            let res = decode(&seal_json(json, 1), PW);
            // A non-object / non-JSON body is Corrupt; a body without "format" is FormatMismatch.
            assert!(
                matches!(res, Err(BackupError::Corrupt(_)) | Err(BackupError::FormatMismatch)),
                "{} -> {:?}",
                json,
                res.map(|_| ())
            );
        }
    }

    // A prompt whose name or category the exe payload cannot carry (line
    // break, NUL, `:` in the category) or whose text is over the ceiling is
    // refused at import: saved as is, it would break the prompts block.
    #[test]
    fn prompt_the_note_cannot_hold_is_corrupt() {
        let big = "x".repeat(crate::features::prompts::MAX_PROMPT_TEXT + 1);
        for (name, text, category) in [
            ("a\nb", "t", "c"),
            ("a\rb", "t", "c"),
            ("a\u{0}b", "t", "c"),
            ("n", "t", "a:b"),
            ("n", "t", "a\nb"),
            ("n", big.as_str(), "c"),
        ] {
            let mut entry = Map::new();
            entry.insert("name".into(), Value::String(name.into()));
            entry.insert("text".into(), Value::String(text.into()));
            entry.insert("category".into(), Value::String(category.into()));
            let json = serde_json::json!({ "format": 1, "prompts": [Value::Object(entry)] }).to_string();
            let res = decode(&seal_json(&json, 1), PW);
            assert!(matches!(res, Err(BackupError::Corrupt(_))), "{:?} {:?} -> {:?}", name, category, res.map(|_| ()));
        }
        // An empty name, text or category is a structurally valid prompt.
        let json = r#"{"format":1,"prompts":[{"name":"","text":"","category":""}]}"#;
        assert_eq!(decode(&seal_json(json, 1), PW).unwrap().prompts, vec![PromptEntry::new("", "", "")]);
    }

    #[test]
    fn json_escaping_matches_serde() {
        // The hand-written writer must agree with a real JSON parser.
        for s in ["", "plain", "q\"b\\s\n\r\t", "\u{0}\u{1f}", "é漢😀", "[TAB:x]"] {
            let mut out = String::new();
            push_json_str(&mut out, s);
            let parsed: String = serde_json::from_str(&out).unwrap();
            assert_eq!(parsed, s);
        }
    }

    /// Frozen format-1 file, written once by the 1.2.17 code and never
    /// regenerated. If this fails, a change broke reading of existing backups:
    /// fix the reader, do NOT touch the fixture.
    #[test]
    fn frozen_v1_fixture_still_decodes() {
        let bytes = include_bytes!("fixtures/v1_basic.lnbak");
        assert_eq!(&bytes[..MAGIC.len()], MAGIC);
        assert_eq!(peek_format(bytes), Ok(1));
        let b = decode(bytes, "fixture-v1").unwrap();

        assert_eq!(
            tab_view(&b.tabs),
            vec![
                ("Alpha".to_string(), "first line\nsecond [TAB:x] line".to_string(), None),
                ("Beta".to_string(), "caf\u{e9} \u{6f22}".to_string(), Some("blue".to_string())),
            ]
        );
        assert_eq!(b.snapshots, vec![Snapshot::with_name(1_700_000_000, "v1", "old text")]);
        assert_eq!(b.secrets, vec![SecretEntry::new("gmail", "multi\nline", "Email")]);
        assert_eq!(b.attachments, vec![Attachment::new("a.bin", vec![0, 1, 2, 255, 254])]);
        assert_eq!(b.created_unix, 1_760_000_000);
        assert_eq!(b.app_version, "1.2.17");
        assert!(b.prompts.is_empty(), "written before issue 058");
        assert_eq!(b.settings.theme, crate::settings::ThemeChoice::Light);
        assert_eq!(b.settings.auto_lock, 7);
        assert_eq!(b.settings.vault_categories, vec!["Work".to_string(), "Home".to_string()]);
    }

    /// Frozen format-1 file with EVERY setting set (TOTP included), written
    /// once and never regenerated. Values are written out on purpose: if a key
    /// of `Settings` is renamed or re-ranged, old backups would be read with
    /// other values and this fails. Fix `Settings` parsing (keep reading the
    /// old keys), do NOT touch the fixture.
    #[test]
    fn frozen_v1_all_settings_fixture_still_decodes() {
        use crate::settings::{CloseAction, Language, ThemeChoice};
        let bytes = include_bytes!("fixtures/v1_all_settings.lnbak");
        let b = decode(bytes, "fixture-v1-settings").unwrap();
        let s = &b.settings;

        assert_eq!(s.save_on_close, CloseAction::Always);
        assert_eq!(s.theme, ThemeChoice::Light);
        assert!(!s.word_wrap);
        assert!(s.minimize_to_tray);
        assert_eq!(s.min_password_length, 11);
        assert_eq!(s.font_family, "Cascadia Mono");
        assert_eq!(s.font_size, 13.5);
        assert_eq!(s.save_key, 0x44);
        assert_eq!(s.save_modifiers, 0x06);
        assert_eq!(s.language, Language::De);
        assert_eq!(s.auto_lock, 3);
        assert_eq!(s.clipboard_clear, 12);
        assert_eq!(s.auto_save, 45);
        assert!(!s.show_line_numbers);
        assert!(s.totp_2fa);
        assert_eq!(s.totp_2fa_secret, "JBSWY3DPEHPK3PXP");
        let totp: Vec<(&str, &str)> = s.totp_entries.iter().map(|e| (e.name.as_str(), e.secret.as_str())).collect();
        assert_eq!(totp, vec![("github", "GEZDGNBVGY3TQOJQ"), ("aws", "MFRGGZDFMZTWQ2LK")]);
        assert_eq!(s.vault_categories, vec!["Work".to_string(), "Home".to_string()]);
        assert_eq!(s.hotkey_modifiers, 3);
        assert_eq!(s.hotkey_key, 77);
        assert_eq!(s.max_attachment_size_mb, 7);
        assert_eq!(s.max_total_attachments_mb, 33);
        // Written before the password generator existed: its defaults apply.
        assert_eq!(s.passgen, crate::features::passgen::Options::default());
        // Machine state was reset at export; the side panel widths did not
        // exist yet and take their defaults.
        assert_eq!(s.active_tab, 0);
        assert_eq!((s.vault_panel_width, s.totp_panel_width, s.attach_panel_width), (300, 280, 280));
        assert_eq!(s.prompts_panel_width, 320);
        assert!(!s.window_maximized);
        assert_eq!(s.window_x, i32::MIN);

        assert_eq!(b.secrets, vec![SecretEntry::in_category("k", "v", "Work")]);
        assert_eq!(tab_view(&b.tabs), vec![("Only".to_string(), "x".to_string(), None)]);
        assert!(b.prompts.is_empty(), "written before issue 058");
    }

    /// Frozen format-1 file with prompts (issue 058), written once by
    /// `json_tool` pack (1.2.25-dev code) and never regenerated. Same rule as
    /// the other fixtures: fix the reader, do NOT touch the fixture.
    #[test]
    fn frozen_v1_prompts_fixture_still_decodes() {
        let bytes = include_bytes!("fixtures/v1_prompts.lnbak");
        assert_eq!(peek_format(bytes), Ok(1));
        let b = decode(bytes, "fixture-v1-prompts").unwrap();
        assert_eq!(
            b.prompts,
            vec![
                PromptEntry::new("Résumé", "Résume ce texte :\n\n- en 3 points\n- en français", "Écriture"),
                PromptEntry::new("host:port review", "Line 1\r\n[/PROMPT]\r\n[LOCKNOTE_VAULT]\r\n\tindented\r\n", "Code"),
                PromptEntry::new("Empty-ish", " ", "General"),
            ]
        );
        assert_eq!(tab_view(&b.tabs), vec![("Note 1".to_string(), "prompts fixture".to_string(), None)]);
        assert_eq!(b.secrets, vec![SecretEntry::in_category("k", "v", "General")]);
        assert!(b.snapshots.is_empty() && b.attachments.is_empty());
        assert_eq!(b.created_unix, 1_763_500_000);
        assert_eq!(b.app_version, "1.2.25-dev");
    }

    /// A scratch directory holding a staging directory and the path of an
    /// exe, for the frozen exe images below. Removed on drop.
    struct ExeDir {
        root: std::path::PathBuf,
        staging: std::path::PathBuf,
        exe: std::path::PathBuf,
    }

    impl Drop for ExeDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn exe_dir(tag: &str, exe_name: &str, image: &[u8]) -> ExeDir {
        let root = std::env::temp_dir().join(format!("ln-frozen-exe-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let staging = root.join("staging");
        let app = root.join("app");
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::create_dir_all(&app).unwrap();
        let exe = app.join(exe_name);
        std::fs::write(&exe, image).unwrap();
        ExeDir { root, staging, exe }
    }

    const FROZEN_1_2_27_EXE: &[u8] = include_bytes!("fixtures/v1_2_27_exe_payload.bin");
    const FROZEN_1_2_27_TMP: &[u8] = include_bytes!("fixtures/v1_2_27_tmp_payload.bin");
    const FROZEN_1_2_27_PW: &str = "fixture-1.2.27";
    /// Public line of the Ed25519 key generated (`ssh_keys::generate`) into
    /// the 1.2.27 fixtures. A throwaway key, only ever used there.
    const FROZEN_1_2_27_SSH_PUB: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIMbWMh39NoIXDGk17oFOsd4Nm6F1u5rioWBNeJg6+8H/ fixture@locknote";

    /// Every block of the notes saved in the 1.2.27 fixtures but the tabs
    /// and the vault (they differ between the exe and the .tmp).
    fn assert_frozen_1_2_27_blocks(b: &Backup) {
        use crate::settings::{CloseAction, Language, ThemeChoice, TotpEntry};
        assert_eq!(
            b.prompts,
            vec![PromptEntry::new("R\u{e9}sum\u{e9}", "R\u{e9}sume ce texte :\n\n- en 3 points", "\u{c9}criture")]
        );
        assert_eq!(b.attachments, vec![Attachment::new("a.bin", vec![0, 1, 2, 255, 254])]);
        // 1.2.27 writes no history block (the safety backups replaced it).
        assert!(b.snapshots.is_empty());

        let s = &b.settings;
        assert_eq!(s.save_on_close, CloseAction::Always);
        assert_eq!(s.theme, ThemeChoice::Light);
        assert!(!s.word_wrap);
        assert!(s.minimize_to_tray);
        assert_eq!(s.min_password_length, 8);
        assert_eq!(s.font_family, "Cascadia Mono");
        assert_eq!(s.font_size, 12.5);
        assert_eq!(s.language, Language::Fr);
        assert_eq!(s.auto_lock, 7);
        assert_eq!(s.clipboard_clear, 12);
        assert_eq!(s.auto_save, 45);
        assert!(!s.show_line_numbers);
        assert_eq!(s.totp_entries, vec![TotpEntry { name: "github".into(), secret: "GEZDGNBVGY3TQOJQ".into() }]);
        assert!(!s.totp_2fa);
        assert_eq!(
            s.vault_categories,
            vec!["General".to_string(), "Work".to_string(), crate::features::ssh_keys::VAULT_CATEGORY.to_string()]
        );
        assert_eq!((s.hotkey_modifiers, s.hotkey_key), (3, 77));
        assert_eq!((s.max_attachment_size_mb, s.max_total_attachments_mb), (7, 33));
        let mut passgen = crate::features::passgen::Options::default();
        passgen.length = 24;
        passgen.symbols = false;
        assert_eq!(s.passgen, passgen);
        // Machine and session state were saved (window, active tab, panel
        // widths, caret, ssh-agent key) and are not imported.
        assert_eq!(s.window_x, i32::MIN);
        assert_eq!(s.active_tab, 0);
        assert_eq!((s.vault_panel_width, s.prompts_panel_width), (300, 320));
    }

    /// The SSH key of the fixtures: private key and public line in the
    /// `SSH Keys` category, the public line still derived from the private key.
    fn assert_frozen_1_2_27_ssh_key(secrets: &[SecretEntry]) {
        let cat = crate::features::ssh_keys::VAULT_CATEGORY;
        let private = secrets.iter().find(|e| e.name == "id_fixture").expect("private key in the vault");
        assert_eq!(private.category, cat);
        assert!(private.value.starts_with("-----BEGIN OPENSSH PRIVATE KEY-----"));
        assert_eq!(crate::features::ssh_keys::public_line_from_private(&private.value, "").unwrap(), FROZEN_1_2_27_SSH_PUB);
        assert_eq!(
            secrets.iter().find(|e| e.name == "id_fixture.pub"),
            Some(&SecretEntry::in_category("id_fixture.pub", FROZEN_1_2_27_SSH_PUB, cat))
        );
    }

    /// Frozen exe image written once by the 1.2.27 code (issue 059) and never
    /// regenerated: a fake program (a few bytes) followed by the notes, as
    /// the public writer of `storage` stages them (`open_session_in`, then
    /// `write_data_in` with the payload built like `EditorForm::on_save`:
    /// `payload::compose`, `Settings::serialize`, `compression::compress`,
    /// `crypto::encrypt` at the default cost). Read here by the path of File >
    /// Import from another LockNote, without the UI: `storage::read_data_in`
    /// then `decode_exe_payload`. If this fails, fix the reader, do NOT touch
    /// the fixture: it is how the 1.2.27 installations migrate to 2.0.
    #[test]
    fn frozen_1_2_27_exe_imports_everything() {
        let d = exe_dir("1227", "LockNote-Pro.exe", FROZEN_1_2_27_EXE);
        let payload = crate::storage::read_data_in(&d.staging, &d.exe).unwrap().expect("notes in the exe");
        let b = decode_exe_payload(&payload, FROZEN_1_2_27_PW).unwrap();

        assert_eq!(
            tab_view(&b.tabs),
            vec![
                ("Courses".to_string(), "lait\n\u{153}ufs \u{6f22}".to_string(), None),
                ("Travail".to_string(), "r\u{e9}union 10h\n[TAB:x] pas un onglet".to_string(), Some("blue".to_string())),
            ]
        );
        assert_eq!(b.secrets.len(), 3);
        assert_eq!(b.secrets[0], SecretEntry::in_category("gmail", "s3cret\nligne 2", "Work"));
        assert_frozen_1_2_27_ssh_key(&b.secrets);
        assert_frozen_1_2_27_blocks(&b);
        assert_eq!(decode_exe_payload(&payload, "fixture-1.2.26").unwrap_err(), BackupError::WrongPasswordOrCorrupt);
    }

    /// Companion written by the 1.2.27 code next to `FROZEN_1_2_27_TMP`
    /// (`write_data_in`), copied as it was. Only the `exe=` path is replaced
    /// (`{exe}`): the format holds the absolute path of the exe, which is the
    /// scratch directory of each run. `base` is the SHA-256 of the notes of
    /// `FROZEN_1_2_27_EXE`; `volume` is the serial of the volume it was
    /// written on (only the "Temporary files" dialog reads it).
    const FROZEN_1_2_27_COMPANION: &str = "LockNote staging companion 1\n\
        exe={exe}\n\
        base=d9511e880201bd8921c8c330853c9dc2b5df841175faa741aed867bc75043993\n\
        program=exe\n\
        volume=fcbf08c1\n";

    /// The state of the production installations: the exe of the fixture
    /// above, and a newer save still pending in its .tmp (frozen image of a
    /// later `write_data_in` on that exe, never regenerated) with the
    /// companion that save wrote (frozen text). The .tmp must win.
    #[test]
    fn frozen_1_2_27_pending_tmp_wins_over_the_exe() {
        use sha2::Digest;
        let d = exe_dir("1227-tmp", "LockNote-Pro.exe", FROZEN_1_2_27_EXE);
        let marker = crate::storage::find_marker(FROZEN_1_2_27_EXE).expect("marker in the frozen exe");
        let exe_notes = &FROZEN_1_2_27_EXE[marker + crate::storage::get_marker_for_update().len()..];
        let base_hex: String = sha2::Sha256::digest(exe_notes).iter().map(|b| format!("{:02x}", b)).collect();
        assert!(FROZEN_1_2_27_COMPANION.contains(&format!("\nbase={}\n", base_hex)), "the frozen companion is based on the frozen exe");
        std::fs::write(crate::storage::get_tmp_path_in(&d.staging, &d.exe), FROZEN_1_2_27_TMP).unwrap();
        let companion = FROZEN_1_2_27_COMPANION.replace("{exe}", &d.exe.display().to_string());
        std::fs::write(crate::storage::companion_path_in(&d.staging, &d.exe), companion).unwrap();
        assert_eq!(
            crate::storage::pending_decision_in(&d.staging, &d.exe),
            crate::storage::PendingDecision::Promote
        );

        let payload = crate::storage::read_data_in(&d.staging, &d.exe).unwrap().expect("notes in the .tmp");
        let b = decode_exe_payload(&payload, FROZEN_1_2_27_PW).unwrap();
        assert_eq!(
            tab_view(&b.tabs),
            vec![
                ("Courses".to_string(), "lait\n\u{153}ufs \u{6f22}".to_string(), None),
                ("Travail".to_string(), "r\u{e9}union 10h\n[TAB:x] pas un onglet".to_string(), Some("blue".to_string())),
                ("Nouveau".to_string(), "\u{e9}crit apr\u{e8}s".to_string(), None),
            ]
        );
        assert_eq!(b.secrets.len(), 3);
        assert_eq!(b.secrets[0], SecretEntry::in_category("gmail", "n0uveau", "Work"));
        assert_frozen_1_2_27_ssh_key(&b.secrets);
        assert_frozen_1_2_27_blocks(&b);
    }

    const FROZEN_1_2_1_EXE: &[u8] = include_bytes!("fixtures/v1_2_1_exe_payload.bin");
    const FROZEN_1_2_1_PW: &str = "fixture-1.2.1";
    const FROZEN_1_2_1_NOTE: &str = "Note publique 1.2.1\nligne 2 : caf\u{e9} \u{6f22}\n[TAB:x] pas un onglet\n";

    /// Frozen exe image written once by the code of the former public
    /// LockNote 1.2.1 (repository `ctardy/LockNote`, in a disposable copy):
    /// `Settings::serialize`, `crypto::encrypt`, `storage::write_data` (the
    /// .tmp it wrote). No compression, no block, one note. The 1.2.1
    /// installations update themselves to 2.0 and keep this payload, so 2.0
    /// must read it: at startup (decrypt, decompress, `Settings::parse`,
    /// `payload::parse_payload`, as `ui::run_session` and the editor do) and
    /// through File > Import from another LockNote. Never regenerated: fix
    /// the reader, do NOT touch the fixture.
    #[test]
    fn frozen_1_2_1_public_exe_is_read() {
        use crate::settings::{CloseAction, ThemeChoice};
        let d = exe_dir("121", "LockNote.exe", FROZEN_1_2_1_EXE);
        let payload = crate::storage::read_data_in(&d.staging, &d.exe).unwrap().expect("notes in the exe");

        // Startup after the update.
        let (hint, encrypted) = crate::storage::split_hint_and_encrypted(&payload);
        assert_eq!(hint, "");
        let text = crate::features::compression::decompress(&crate::crypto::decrypt(&encrypted, FROZEN_1_2_1_PW).unwrap());
        let (s, note_text) = Settings::parse(&text);
        assert_eq!(s.save_on_close, CloseAction::Always);
        assert_eq!(s.theme, ThemeChoice::Light);
        assert!(!s.word_wrap);
        assert!(s.minimize_to_tray);
        assert_eq!(s.min_password_length, 5);
        assert_eq!(s.font_family, "Cascadia Mono");
        assert_eq!(s.font_size, 13.5);
        assert_eq!((s.save_key, s.save_modifiers), (0x44, 0x06));
        assert_eq!((s.window_width, s.window_height, s.window_x, s.window_y), (1000, 700, 120, 80));
        assert!(!s.window_maximized);
        // Keys 1.2.1 never wrote take the values `Settings::parse` falls
        // back to.
        let (fallback, _) = Settings::parse("");
        assert_eq!((s.language, s.auto_lock, s.clipboard_clear, s.auto_save), (fallback.language, 15, 30, 30));
        assert_eq!(s.totp_entries, fallback.totp_entries);
        assert_eq!(s.vault_categories, fallback.vault_categories);
        let parsed = crate::features::payload::parse_payload(
            &note_text,
            &crate::features::vault::categories::effective(&s.vault_categories),
        );
        assert_eq!(
            tab_view(&parsed.tabs),
            vec![(crate::features::payload::DEFAULT_TAB_NAME.to_string(), FROZEN_1_2_1_NOTE.to_string(), None)]
        );
        assert!(parsed.session.is_none());
        assert!(parsed.secrets.is_empty() && parsed.prompts.is_empty() && parsed.attachments.is_empty());

        // Import from another LockNote.
        let b = decode_exe_payload(&payload, FROZEN_1_2_1_PW).unwrap();
        assert_eq!(tab_view(&b.tabs), tab_view(&parsed.tabs));
        assert_eq!(b.settings.theme, ThemeChoice::Light);
        assert_eq!(b.settings.min_password_length, 5);
        assert_eq!(b.settings.window_x, i32::MIN, "machine state is not imported");
        assert!(b.secrets.is_empty() && b.prompts.is_empty() && b.attachments.is_empty() && b.snapshots.is_empty());
    }

    #[test]
    fn the_2fa_and_totp_secrets_survive_export_import_and_exe_migration() {
        let mut b = sample();
        b.settings.totp_entries = vec![
            crate::settings::TotpEntry { name: "github".into(), secret: "GEZDGNBVGY3TQOJQ".into() },
            crate::settings::TotpEntry { name: "aws".into(), secret: "MFRGGZDFMZTWQ2LK".into() },
        ];
        let before = b.settings.clone();
        let bytes = encode(&b, PW, FAST).unwrap();
        // The live settings handed to the export are untouched.
        assert!(b.settings == before);
        assert_eq!(b.settings.totp_2fa_secret, "JBSWY3DPEHPK3PXP");
        let back = decode(&bytes, PW).unwrap();
        assert_eq!(back.settings.totp_2fa_secret, "JBSWY3DPEHPK3PXP");
        assert_eq!(back.settings.totp_entries, b.settings.totp_entries);
        for (hint, compress) in [(None, true), (Some("h"), false)] {
            let got = decode_exe_payload(&exe_payload(&b, hint, compress), PW).unwrap();
            assert!(got.settings.totp_2fa);
            assert_eq!(got.settings.totp_2fa_secret, "JBSWY3DPEHPK3PXP");
            assert_eq!(got.settings.totp_entries, b.settings.totp_entries);
            // And it exports again with its secrets.
            let again = decode(&encode(&got, PW, FAST).unwrap(), PW).unwrap();
            assert_eq!(again.settings.totp_2fa_secret, "JBSWY3DPEHPK3PXP");
            assert_eq!(again.settings.totp_entries, b.settings.totp_entries);
        }
    }
}
