// Feature modules (those of the former Pro edition, always built since 2.0)

// Reserved markers of the payload and the escaping shared by every block
// that holds user text (issue 050).
pub mod markers;

pub mod tabs;

pub mod vault;

// Stored prompts, copied from the Prompts panel (issue 058).
pub mod prompts;

pub mod totp;

pub mod compression;

pub mod i18n;

pub mod autolock;

pub mod clipboard_guard;

pub mod autosave;

pub mod hotkey;

pub mod snapshots;

pub mod attachments;

pub mod stego;

pub mod ssh_keys;

// Password generator (Tools menu and the vault's secret dialog).
pub mod passgen;

// Persistence lock — autosave/autolock/unlock all coordinate writes via a
// single mutex.
pub mod persistence_lock;

// Session state (active tab + caret line); the block is
// optional and absence is treated as "no prior session".
pub mod session_state;

// The decrypted note payload: `compose` builds it (save), `parse_payload`
// takes it apart (startup + unlock).
pub mod payload;

// Full backup (export / import of tabs, vault, snapshots, attachments, settings).
pub mod backup;

// Automatic safety backups before an import or an update, with a retention
// policy, see auto_backup/mod.rs.
pub mod auto_backup;

// Import log: one line per import step (no secrets), see import_log/mod.rs.
pub mod import_log;

// Markdown headings of a tab, for Edit > Go to Heading (Ctrl+Shift+O).
pub mod outline;

// Line diff of a tab against its last save, for Tab > Compare with Last Save.
pub mod line_diff;
