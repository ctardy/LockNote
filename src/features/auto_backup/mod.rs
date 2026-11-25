// Automatic safety backups — a `.lnbak` written before a destructive action
// (full import, import from another exe, update) and purged by a fixed policy.
//
// Directory: %LOCALAPPDATA%\LockNote\auto-backups (OS temp dir when the
// variable is missing, like the staging file and the import log).
//
// File names: `pre-import-<unix>.lnbak`, `pre-update-<unix>.lnbak` and
// `manual-<unix>.lnbak` (forced by the user, never purged by the policy). The
// name is the whole contract of what may be purged: a file that does not match
// it exactly is never touched.
//
// Retention (see docs/SPEC.md "Automatic safety backups"):
//   * the `KEEP_NEWEST` newest files of each automatic kind are never purged;
//     `manual` files are never purged at all (they still count in the size);
//   * beyond those, a file older than `MAX_AGE_SECS` is purged;
//   * beyond those, the oldest are purged until the total fits `MAX_TOTAL_BYTES`.
//     The protected files are never purged for size: the cap can stay exceeded
//     and the report says so.
//
// Purging only runs after a new backup was written and verified, so the number
// of valid backups never drops because of a failed write.
//
// The file is the one `ui::editor::write_backup_file` produces (encrypted with
// the note password, read back and decoded before it gets its name). This
// module decides where and when, not how.

use std::path::{Path, PathBuf};

/// Directory name under `%LOCALAPPDATA%\LockNote`.
pub const DIR_NAME: &str = "auto-backups";
/// Newest files kept per kind whatever their age or size.
pub const KEEP_NEWEST: usize = 3;
/// Age after which an unprotected file is purged (30 days).
pub const MAX_AGE_SECS: u64 = 30 * 24 * 3600;
/// Total size above which the oldest unprotected files are purged (1 GiB).
pub const MAX_TOTAL_BYTES: u64 = 1024 * 1024 * 1024;
/// A leftover `*.tmp` of a crashed write older than this is removed (1 hour).
pub const STALE_TMP_SECS: u64 = 3600;

const EXTENSION: &str = ".lnbak";

/// What the backup protects against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    PreImport,
    PreUpdate,
    /// Forced by the user ("Create a backup now"). Never purged by the policy.
    Manual,
}

impl Kind {
    fn prefix(self) -> &'static str {
        match self {
            Kind::PreImport => "pre-import-",
            Kind::PreUpdate => "pre-update-",
            Kind::Manual => "manual-",
        }
    }

    /// Name used in log lines.
    pub fn tag(self) -> &'static str {
        match self {
            Kind::PreImport => "pre_import",
            Kind::PreUpdate => "pre_update",
            Kind::Manual => "manual",
        }
    }
}

/// One file of the directory, as the purge policy sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub kind: Kind,
    pub unix: u64,
    pub size: u64,
}

/// What a purge did. `failed` files are still on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PurgeReport {
    pub removed: usize,
    pub failed: usize,
    /// The protected files alone exceed `MAX_TOTAL_BYTES`.
    pub over_cap: bool,
}

/// Where the backups live.
pub fn dir_path() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    base.join("LockNote").join(DIR_NAME)
}

/// `pre-import-1700000000.lnbak`.
pub fn file_name(kind: Kind, unix: u64) -> String {
    format!("{}{}{}", kind.prefix(), unix, EXTENSION)
}

/// Inverse of `file_name`. `None` for anything else, including a name with a
/// path, another case, a non-numeric stamp or a trailing suffix.
pub fn parse_name(name: &str) -> Option<(Kind, u64)> {
    for kind in [Kind::PreImport, Kind::PreUpdate, Kind::Manual] {
        let Some(rest) = name.strip_prefix(kind.prefix()) else { continue };
        let Some(stamp) = rest.strip_suffix(EXTENSION) else { continue };
        if stamp.is_empty() || !stamp.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        // An overflowing stamp is not ours either.
        return stamp.parse::<u64>().ok().map(|unix| (kind, unix));
    }
    None
}

/// `pre-import-<unix>.lnbak.<pid>.tmp`: the temporary file `write_backup_file`
/// leaves behind when the process dies between create and rename.
fn is_tmp_name(name: &str) -> bool {
    let Some(base) = name.strip_suffix(".tmp") else { return false };
    let Some((base, pid)) = base.rsplit_once('.') else { return false };
    !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit()) && parse_name(base).is_some()
}

/// Names to remove, per the retention policy. Pure: the caller does the I/O.
pub fn plan_purge(entries: &[Entry], now: u64) -> (Vec<String>, bool) {
    // Newest first inside each kind; the first KEEP_NEWEST are protected.
    let mut protected: Vec<&Entry> = Vec::new();
    let mut open: Vec<&Entry> = Vec::new();
    // Forced by the user: counted in the total, never removed by the policy.
    protected.extend(entries.iter().filter(|e| e.kind == Kind::Manual));
    for kind in [Kind::PreImport, Kind::PreUpdate] {
        let mut of_kind: Vec<&Entry> = entries.iter().filter(|e| e.kind == kind).collect();
        of_kind.sort_by(|a, b| b.unix.cmp(&a.unix).then_with(|| b.name.cmp(&a.name)));
        let split = KEEP_NEWEST.min(of_kind.len());
        protected.extend_from_slice(&of_kind[..split]);
        open.extend_from_slice(&of_kind[split..]);
    }

    let mut doomed: Vec<String> = Vec::new();
    let mut kept: Vec<&Entry> = Vec::new();
    for e in open {
        // A file dated in the future has age 0: never "old".
        if now.saturating_sub(e.unix) > MAX_AGE_SECS {
            doomed.push(e.name.clone());
        } else {
            kept.push(e);
        }
    }

    // Size cap: oldest unprotected first, across both kinds.
    kept.sort_by(|a, b| a.unix.cmp(&b.unix).then_with(|| a.name.cmp(&b.name)));
    let mut total: u64 = protected.iter().chain(kept.iter()).map(|e| e.size).sum();
    for e in kept {
        if total <= MAX_TOTAL_BYTES {
            break;
        }
        total -= e.size;
        doomed.push(e.name.clone());
    }
    (doomed, total > MAX_TOTAL_BYTES)
}

/// What `dir` holds that belongs to this module. A missing directory is empty.
struct Scan {
    backups: Vec<Entry>,
    /// `(name, modified unix time)`; 0 when the time is unreadable.
    tmps: Vec<(String, u64)>,
}

fn scan(dir: &Path) -> std::io::Result<Scan> {
    let mut out = Scan { backups: Vec::new(), tmps: Vec::new() };
    let read = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(e),
    };
    for item in read {
        let item = item?;
        let Ok(name) = item.file_name().into_string() else { continue };
        let meta = item.metadata()?;
        if !meta.is_file() {
            continue;
        }
        if let Some((kind, unix)) = parse_name(&name) {
            out.backups.push(Entry { name, kind, unix, size: meta.len() });
        } else if is_tmp_name(&name) {
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            out.tmps.push((name, modified));
        }
    }
    Ok(out)
}

fn remove_all(dir: &Path, names: &[String], report: &mut PurgeReport) {
    for name in names {
        match std::fs::remove_file(dir.join(name)) {
            Ok(()) => report.removed += 1,
            Err(_) => report.failed += 1,
        }
    }
}

/// Apply the retention policy to `dir`, and remove stale `.tmp` leftovers of
/// a crashed write. A missing directory is an empty one.
pub fn purge_dir(dir: &Path, now: u64) -> std::io::Result<PurgeReport> {
    let found = scan(dir)?;
    let (mut doomed, over_cap) = plan_purge(&found.backups, now);
    doomed.extend(
        found
            .tmps
            .into_iter()
            .filter(|(_, modified)| now.saturating_sub(*modified) > STALE_TMP_SECS)
            .map(|(name, _)| name),
    );
    let mut report = PurgeReport { over_cap, ..PurgeReport::default() };
    remove_all(dir, &doomed, &mut report);
    Ok(report)
}

/// Remove every automatic backup of `dir` (and their `.tmp`), whatever the
/// policy says. Used when the password changes.
pub fn purge_all(dir: &Path) -> std::io::Result<PurgeReport> {
    let found = scan(dir)?;
    let doomed: Vec<String> = found
        .backups
        .into_iter()
        .map(|e| e.name)
        .chain(found.tmps.into_iter().map(|(name, _)| name))
        .collect();
    let mut report = PurgeReport::default();
    remove_all(dir, &doomed, &mut report);
    Ok(report)
}

/// Number of automatic backups in `dir` (`.tmp` leftovers excluded).
pub fn count(dir: &Path) -> usize {
    scan(dir).map(|s| s.backups.len()).unwrap_or(0)
}

/// Every automatic backup of `dir`, newest first (`.tmp` leftovers excluded).
/// A missing directory is an empty list.
pub fn list(dir: &Path) -> std::io::Result<Vec<Entry>> {
    let mut entries = scan(dir)?.backups;
    entries.sort_by(|a, b| b.unix.cmp(&a.unix).then_with(|| b.name.cmp(&a.name)));
    Ok(entries)
}

/// Delete one backup by name. Refuses (`InvalidInput`) a name that is not an
/// automatic backup name, so a crafted name cannot reach another file.
pub fn delete(dir: &Path, name: &str) -> std::io::Result<()> {
    if parse_name(name).is_none() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "not an automatic backup name",
        ));
    }
    std::fs::remove_file(dir.join(name))
}

/// Size for the list: `512 B`, `1.5 KiB`, `12.3 MiB`, `1.00 GiB`.
pub fn format_size(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    const GIB: u64 = MIB * 1024;
    if bytes < KIB {
        format!("{} B", bytes)
    } else if bytes < MIB {
        format!("{:.1} KiB", bytes as f64 / KIB as f64)
    } else if bytes < GIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else {
        format!("{:.2} GiB", bytes as f64 / GIB as f64)
    }
}

/// Error of `store`: `(stage, detail)`, the shape `write_backup_file` uses.
pub type StoreError = (&'static str, String);

/// Create `dir`, pick a free name for `kind` and let `write` put the backup
/// there. Only when `write` succeeds is the directory purged. Returns the
/// path written and the purge report.
///
/// `write` receives the final path and must not leave anything under it on
/// failure (`write_backup_file` writes a temporary file, verifies it, renames).
pub fn store(
    dir: &Path,
    kind: Kind,
    now: u64,
    write: impl FnOnce(&Path) -> Result<usize, StoreError>,
) -> Result<(PathBuf, PurgeReport), StoreError> {
    std::fs::create_dir_all(dir).map_err(|e| ("dir", e.to_string()))?;

    // `now + i`: two backups in the same second get two names, never a replace.
    let path = (0..1000u64)
        .map(|i| dir.join(file_name(kind, now + i)))
        .find(|p| !p.exists())
        .ok_or(("dir", "no free file name".to_string()))?;

    write(&path)?;
    if !path.is_file() {
        return Err(("write", "the backup was not written".to_string()));
    }

    // The backup is safe on disk; a purge that cannot even list the directory
    // is reported, not fatal.
    let report = purge_dir(dir, now).unwrap_or(PurgeReport { failed: 1, ..PurgeReport::default() });
    Ok((path, report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const DAY: u64 = 24 * 3600;
    const MIB: u64 = 1024 * 1024;
    /// Far enough from the epoch for "100 days old" to be positive.
    const NOW: u64 = 1_800_000_000;

    fn entry(kind: Kind, age_days: u64, size: u64) -> Entry {
        let unix = NOW - age_days * DAY;
        Entry { name: file_name(kind, unix), kind, unix, size }
    }

    fn removed(entries: &[Entry]) -> Vec<String> {
        let (mut names, _) = plan_purge(entries, NOW);
        names.sort();
        names
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ln-autobackup-{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    fn put(dir: &Path, name: &str, len: usize) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join(name), vec![7u8; len]).unwrap();
    }

    fn names_in(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    // ── names ──

    #[test]
    fn name_roundtrip() {
        for kind in [Kind::PreImport, Kind::PreUpdate] {
            let n = file_name(kind, 1_700_000_000);
            assert_eq!(parse_name(&n), Some((kind, 1_700_000_000)));
        }
        assert_eq!(file_name(Kind::PreImport, 5), "pre-import-5.lnbak");
        assert_eq!(file_name(Kind::PreUpdate, 5), "pre-update-5.lnbak");
    }

    #[test]
    fn size_is_shown_in_the_right_unit() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1023), "1023 B");
        assert_eq!(format_size(1024), "1.0 KiB");
        assert_eq!(format_size(1536), "1.5 KiB");
        assert_eq!(format_size(1024 * 1024 - 1), "1024.0 KiB");
        assert_eq!(format_size(12 * MIB + MIB / 4), "12.2 MiB");
        assert_eq!(format_size(1024 * MIB), "1.00 GiB");
        assert_eq!(format_size(u64::MAX), "17179869184.00 GiB");
    }

    #[test]
    fn manual_name_roundtrip() {
        assert_eq!(file_name(Kind::Manual, 5), "manual-5.lnbak");
        assert_eq!(parse_name("manual-5.lnbak"), Some((Kind::Manual, 5)));
        assert_eq!(parse_name("manual-5.lnbak.tmp"), None);
        assert_eq!(parse_name("manual-.lnbak"), None);
    }

    #[test]
    fn only_exact_names_match() {
        for bad in [
            "",
            "user.lnbak",
            "pre-import-.lnbak",
            "pre-import-1a.lnbak",
            "pre-import--1.lnbak",
            "pre-import-1.lnbak.tmp",
            "pre-import-1.lnbak.123.tmp",
            "pre-import-1.LNBAK",
            "PRE-IMPORT-1.lnbak",
            "pre-other-1.lnbak",
            "x-pre-import-1.lnbak",
            "..\\pre-import-1.lnbak",
            "../pre-import-1.lnbak",
            "sub/pre-import-1.lnbak",
            "pre-import-99999999999999999999999.lnbak",
        ] {
            assert_eq!(parse_name(bad), None, "{:?}", bad);
        }
    }

    // ── policy ──

    #[test]
    fn newest_three_survive_whatever_their_age() {
        let e: Vec<Entry> = (100..103).map(|d| entry(Kind::PreImport, d, MIB)).collect();
        assert!(removed(&e).is_empty());
    }

    #[test]
    fn old_files_beyond_the_floor_are_purged() {
        let e: Vec<Entry> = (100..105).map(|d| entry(Kind::PreImport, d, MIB)).collect();
        // Ages 100..=104: the three newest (100, 101, 102) stay.
        let mut want = vec![e[3].name.clone(), e[4].name.clone()];
        want.sort();
        assert_eq!(removed(&e), want);
    }

    #[test]
    fn recent_files_beyond_the_floor_are_kept() {
        let e: Vec<Entry> = (1..=6).map(|d| entry(Kind::PreImport, d, MIB)).collect();
        assert!(removed(&e).is_empty());
    }

    #[test]
    fn age_limit_is_thirty_days_exactly() {
        let mut e: Vec<Entry> = (1..=3).map(|d| entry(Kind::PreImport, d, MIB)).collect();
        e.push(entry(Kind::PreImport, 30, MIB)); // 30 days old: kept
        e.push(entry(Kind::PreImport, 31, MIB)); // 31 days old: purged
        assert_eq!(removed(&e), vec![e[4].name.clone()]);
    }

    #[test]
    fn kinds_are_counted_apart() {
        // Ten recent imports must not push the only (old) update out.
        let mut e: Vec<Entry> = (1..=10).map(|d| entry(Kind::PreImport, d, MIB)).collect();
        e.push(entry(Kind::PreUpdate, 200, MIB));
        assert!(!removed(&e).contains(&e[10].name));
    }

    #[test]
    fn size_cap_purges_oldest_unprotected_first() {
        // 5 imports of 300 MiB: 1.5 GiB. The two unprotected, oldest first.
        let e: Vec<Entry> = (1..=5).map(|d| entry(Kind::PreImport, d, 300 * MIB)).collect();
        let (names, over) = plan_purge(&e, NOW);
        assert!(!over);
        let mut got = names;
        got.sort();
        let mut want = vec![e[3].name.clone(), e[4].name.clone()];
        want.sort();
        assert_eq!(got, want);
    }

    #[test]
    fn size_cap_stops_as_soon_as_it_fits() {
        // 4 x 300 MiB = 1.2 GiB: dropping the single oldest is enough.
        let e: Vec<Entry> = (1..=4).map(|d| entry(Kind::PreImport, d, 300 * MIB)).collect();
        assert_eq!(removed(&e), vec![e[3].name.clone()]);
    }

    #[test]
    fn size_cap_never_touches_protected_files() {
        // 6 x 300 MiB, three per kind: all protected, cap exceeded, nothing goes.
        let mut e: Vec<Entry> = (1..=3).map(|d| entry(Kind::PreImport, d, 300 * MIB)).collect();
        e.extend((1..=3).map(|d| entry(Kind::PreUpdate, d, 300 * MIB)));
        let (names, over) = plan_purge(&e, NOW);
        assert!(names.is_empty());
        assert!(over, "the cap is exceeded and the report must say so");
    }

    #[test]
    fn size_cap_spans_both_kinds_oldest_first() {
        // 3 + 3 protected at 100 MiB, plus an old import and a newer update of
        // 300 MiB each: 1.2 GiB. The oldest unprotected goes first, and one
        // removal is enough (0.9 GiB), so the newer update stays.
        let mut e: Vec<Entry> = (1..=3).map(|d| entry(Kind::PreImport, d, 100 * MIB)).collect();
        e.extend((1..=3).map(|d| entry(Kind::PreUpdate, d, 100 * MIB)));
        let old_import = entry(Kind::PreImport, 20, 300 * MIB);
        let newer_update = entry(Kind::PreUpdate, 10, 300 * MIB);
        e.push(old_import.clone());
        e.push(newer_update);
        assert_eq!(removed(&e), vec![old_import.name]);
    }

    #[test]
    fn future_dated_file_is_not_purged_as_old() {
        let mut e: Vec<Entry> = (1..=3).map(|d| entry(Kind::PreImport, d, MIB)).collect();
        let future = Entry {
            name: file_name(Kind::PreImport, NOW + 10 * DAY),
            kind: Kind::PreImport,
            unix: NOW + 10 * DAY,
            size: MIB,
        };
        e.push(future);
        assert!(removed(&e).is_empty());
    }

    #[test]
    fn manual_backups_are_never_purged_by_age_or_count() {
        // Ten manual backups, all 200 days old: none goes.
        let e: Vec<Entry> = (200..210).map(|d| entry(Kind::Manual, d, MIB)).collect();
        assert!(removed(&e).is_empty());
    }

    #[test]
    fn manual_backups_do_not_take_the_floor_of_the_others() {
        // Manual files must not count as "newest" of the pre-import pool.
        let mut e: Vec<Entry> = (1..=5).map(|d| entry(Kind::Manual, d, MIB)).collect();
        e.extend((100..105).map(|d| entry(Kind::PreImport, d, MIB)));
        let mut want = vec![e[8].name.clone(), e[9].name.clone()];
        want.sort();
        assert_eq!(removed(&e), want);
    }

    #[test]
    fn manual_backups_count_in_the_size_cap_but_are_never_removed_for_it() {
        // 2 manual x 400 MiB + 4 imports x 200 MiB = 1.6 GiB. The import beyond
        // the floor goes (1.4 GiB, still over): the cap stays exceeded, and the
        // manual files stay.
        let mut e: Vec<Entry> = (1..=2).map(|d| entry(Kind::Manual, d, 400 * MIB)).collect();
        e.extend((1..=4).map(|d| entry(Kind::PreImport, d, 200 * MIB)));
        let (names, over) = plan_purge(&e, NOW);
        assert_eq!(names, vec![e[5].name.clone()]);
        assert!(over);
    }

    // ── directory ──

    #[test]
    fn purge_dir_only_touches_matching_names() {
        let dir = temp_dir("foreign");
        for d in 100..106 {
            put(&dir, &file_name(Kind::PreImport, NOW - d * DAY), 10);
        }
        for foreign in ["user-export.lnbak", "notes.txt", "pre-import-abc.lnbak", "pre-import-1.lnbak.bak"] {
            put(&dir, foreign, 10);
        }
        fs::create_dir_all(dir.join("pre-import-5.lnbak.d")).unwrap();

        let report = purge_dir(&dir, NOW).unwrap();
        assert_eq!(report.removed, 3);
        assert_eq!(report.failed, 0);

        let left = names_in(&dir);
        for foreign in ["user-export.lnbak", "notes.txt", "pre-import-abc.lnbak", "pre-import-1.lnbak.bak", "pre-import-5.lnbak.d"] {
            assert!(left.contains(&foreign.to_string()), "{} was touched", foreign);
        }
        assert_eq!(left.iter().filter(|n| parse_name(n).is_some()).count(), 3);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn purge_dir_on_missing_directory_is_empty() {
        let dir = temp_dir("missing");
        assert_eq!(purge_dir(&dir, NOW).unwrap(), PurgeReport::default());
        assert_eq!(count(&dir), 0);
    }

    #[test]
    fn stale_tmp_is_removed_fresh_tmp_is_kept() {
        let dir = temp_dir("tmp");
        put(&dir, "pre-import-5.lnbak.111.tmp", 10);
        put(&dir, "pre-import-6.lnbak.222.tmp", 10);
        put(&dir, "other.tmp", 10);
        let old = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(NOW - 2 * STALE_TMP_SECS);
        let f = fs::OpenOptions::new().write(true).open(dir.join("pre-import-5.lnbak.111.tmp")).unwrap();
        f.set_modified(old).unwrap();
        drop(f);
        let fresh = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(NOW - 60);
        let f = fs::OpenOptions::new().write(true).open(dir.join("pre-import-6.lnbak.222.tmp")).unwrap();
        f.set_modified(fresh).unwrap();
        drop(f);

        let report = purge_dir(&dir, NOW).unwrap();
        assert_eq!(report.removed, 1);
        let left = names_in(&dir);
        assert!(!left.contains(&"pre-import-5.lnbak.111.tmp".to_string()));
        assert!(left.contains(&"pre-import-6.lnbak.222.tmp".to_string()), "a write may be in progress");
        assert!(left.contains(&"other.tmp".to_string()), "not ours");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn purge_all_removes_every_backup_and_nothing_else() {
        let dir = temp_dir("all");
        put(&dir, &file_name(Kind::PreImport, NOW), 10);
        put(&dir, &file_name(Kind::PreUpdate, NOW), 10);
        put(&dir, "pre-import-5.lnbak.111.tmp", 10);
        put(&dir, "user-export.lnbak", 10);

        assert_eq!(count(&dir), 2);
        let report = purge_all(&dir).unwrap();
        assert_eq!(report.removed, 3);
        assert_eq!(names_in(&dir), vec!["user-export.lnbak".to_string()]);
        assert_eq!(count(&dir), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_is_newest_first_and_ignores_foreign_files() {
        let dir = temp_dir("list");
        put(&dir, &file_name(Kind::PreImport, 100), 10);
        put(&dir, &file_name(Kind::Manual, 300), 30);
        put(&dir, &file_name(Kind::PreUpdate, 200), 20);
        put(&dir, "user-export.lnbak", 5);
        put(&dir, "pre-import-5.lnbak.111.tmp", 5);

        let got = list(&dir).unwrap();
        let view: Vec<(Kind, u64, u64)> = got.iter().map(|e| (e.kind, e.unix, e.size)).collect();
        assert_eq!(
            view,
            vec![(Kind::Manual, 300, 30), (Kind::PreUpdate, 200, 20), (Kind::PreImport, 100, 10)]
        );
        assert!(list(&temp_dir("list-missing")).unwrap().is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_removes_one_backup_and_refuses_any_other_name() {
        let dir = temp_dir("delete");
        let keep = file_name(Kind::PreImport, 1);
        let gone = file_name(Kind::Manual, 2);
        put(&dir, &keep, 10);
        put(&dir, &gone, 10);
        put(&dir, "user-export.lnbak", 10);
        fs::write(dir.parent().unwrap().join(format!("outside-{}.lnbak", std::process::id())), b"x").unwrap();

        delete(&dir, &gone).unwrap();
        assert_eq!(names_in(&dir), vec![keep.clone(), "user-export.lnbak".to_string()]);

        for bad in ["user-export.lnbak", "..\\x.lnbak", "../x.lnbak", "", "pre-import-1.lnbak.tmp"] {
            let e = delete(&dir, bad).unwrap_err();
            assert_eq!(e.kind(), std::io::ErrorKind::InvalidInput, "{:?}", bad);
        }
        // A valid name that does not exist is reported, not ignored.
        assert!(delete(&dir, &file_name(Kind::Manual, 99)).is_err());
        assert_eq!(names_in(&dir), vec![keep, "user-export.lnbak".to_string()]);
        let _ = fs::remove_file(dir.parent().unwrap().join(format!("outside-{}.lnbak", std::process::id())));
        let _ = fs::remove_dir_all(&dir);
    }

    // ── store ──

    #[test]
    fn store_writes_under_the_final_name_then_purges() {
        let dir = temp_dir("store-ok");
        for d in 100..106 {
            put(&dir, &file_name(Kind::PreImport, NOW - d * DAY), 10);
        }
        let (path, report) = store(&dir, Kind::PreImport, NOW, |p| {
            fs::write(p, b"backup").unwrap();
            Ok(6)
        })
        .unwrap();

        assert_eq!(path, dir.join(file_name(Kind::PreImport, NOW)));
        assert_eq!(fs::read(&path).unwrap(), b"backup");
        // New file + the two newest old ones are the floor; the other four go.
        assert_eq!(report.removed, 4);
        assert_eq!(count(&dir), 3);
        let _ = fs::remove_dir_all(&dir);
    }

    /// The file `store` leaves is a real backup: `backup::decode` restores it
    /// with the note password, which is what "Import full backup" does.
    #[test]
    fn stored_file_restores_through_the_normal_import() {
        use crate::features::backup;
        let b = backup::Backup {
            settings: crate::settings::Settings::default(),
            tabs: vec![crate::features::tabs::NoteTab::with_content("Alpha", "keep me", None)],
            snapshots: Vec::new(),
            prompts: vec![crate::features::prompts::PromptEntry::new("Review", "Review this:\n", "Code")],
            secrets: vec![crate::features::vault::SecretEntry::new("gmail", "s3cret", "Email")],
            attachments: Vec::new(),
            created_unix: NOW,
            app_version: "test".into(),
        };
        let dir = temp_dir("restore");
        let (path, _) = store(&dir, Kind::PreImport, NOW, |p| {
            let bytes = backup::encode(&b, "pw", (8, 1, 1)).map_err(|e| ("encode", e.to_string()))?;
            fs::write(p, &bytes).map_err(|e| ("write", e.to_string()))?;
            Ok(bytes.len())
        })
        .unwrap();

        let back = backup::decode(&fs::read(&path).unwrap(), "pw").unwrap();
        assert_eq!(back.tabs[0].content, "keep me");
        assert_eq!(back.secrets, b.secrets);
        assert_eq!(back.prompts, b.prompts);
        assert_eq!(
            backup::decode(&fs::read(&path).unwrap(), "other").unwrap_err(),
            backup::BackupError::WrongPasswordOrCorrupt
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn store_does_not_purge_when_the_write_fails() {
        let dir = temp_dir("store-fail");
        for d in 100..106 {
            put(&dir, &file_name(Kind::PreImport, NOW - d * DAY), 10);
        }
        let before = names_in(&dir);
        let err = store(&dir, Kind::PreImport, NOW, |_| Err(("verify", "boom".to_string()))).unwrap_err();
        assert_eq!(err, ("verify", "boom".to_string()));
        assert_eq!(names_in(&dir), before, "a failed write must not cost an older backup");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn store_never_reuses_a_name() {
        let dir = temp_dir("store-twice");
        let write = |p: &Path| {
            fs::write(p, b"x").unwrap();
            Ok(1)
        };
        let (a, _) = store(&dir, Kind::PreUpdate, NOW, write).unwrap();
        let (b, _) = store(&dir, Kind::PreUpdate, NOW, write).unwrap();
        assert_ne!(a, b);
        assert!(a.exists() && b.exists());
        assert!(parse_name(b.file_name().unwrap().to_str().unwrap()).is_some());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn store_reports_an_unusable_directory() {
        let dir = temp_dir("store-dir");
        fs::create_dir_all(dir.parent().unwrap()).unwrap();
        fs::write(&dir, b"a file where the directory should be").unwrap();
        let err = store(&dir, Kind::PreImport, NOW, |_| Ok(0)).unwrap_err();
        assert_eq!(err.0, "dir");
        let _ = fs::remove_file(&dir);
    }
}
