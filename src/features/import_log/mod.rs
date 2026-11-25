// Import log — one line per step of an import, so a silent failure leaves a trace.
//
// File: %LOCALAPPDATA%\LockNote\import.log (OS temp dir when the variable is
// missing, like the staging file). Append-only, capped: when it grows past
// `MAX_BYTES` the oldest half is dropped.
//
// Confidentiality: callers pass step names, counts, sizes and error variants
// only. Never a password, a path, a note title or any content.

use std::io::Write;
use std::path::{Path, PathBuf};

/// Size above which the log is cut back.
pub const MAX_BYTES: u64 = 64 * 1024;

/// Where the log lives.
pub fn log_path() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    base.join("LockNote").join("import.log")
}

/// Append `line` with a Unix timestamp to the standard log. A failure to write
/// is reported on stderr: logging must never break the import itself.
pub fn note(line: &str) {
    eprintln!("{}", line);
    if let Err(e) = append_to(&log_path(), line, unix_now()) {
        eprintln!("import_log: cannot write: {}", e);
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Append one line to `path`, creating the directory, then trim the file when
/// it is over `MAX_BYTES` (whole lines only).
pub fn append_to(path: &Path, line: &str, now: u64) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // One line per call: newlines in `line` would forge extra entries.
    let clean: String = line.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "[{}] {}", now, clean)?;
    drop(f);
    trim(path)
}

fn trim(path: &Path) -> std::io::Result<()> {
    let len = std::fs::metadata(path)?.len();
    if len <= MAX_BYTES {
        return Ok(());
    }
    let bytes = std::fs::read(path)?;
    let cut = bytes.len() / 2;
    // Start at the next full line.
    let start = bytes[cut..].iter().position(|&b| b == b'\n').map(|i| cut + i + 1).unwrap_or(cut);
    std::fs::write(path, &bytes[start..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ln-importlog-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d.join("sub").join("import.log")
    }

    #[test]
    fn appends_lines_with_timestamp_and_creates_dirs() {
        let p = temp("append");
        append_to(&p, "import_exe: start", 100).unwrap();
        append_to(&p, "import_exe: done", 101).unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "[100] import_exe: start\n[101] import_exe: done\n");
        let _ = std::fs::remove_dir_all(p.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn newlines_cannot_forge_entries() {
        let p = temp("forge");
        append_to(&p, "a\n[999] forged\r\nb", 1).unwrap();
        let text = std::fs::read_to_string(&p).unwrap();
        assert_eq!(text.lines().count(), 1);
        assert!(text.starts_with("[1] a [999] forged"));
        let _ = std::fs::remove_dir_all(p.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn stays_bounded_and_keeps_the_latest_whole_lines() {
        let p = temp("cap");
        let long = "x".repeat(1000);
        for i in 0..200u64 {
            append_to(&p, &format!("{} {}", i, long), i).unwrap();
        }
        let text = std::fs::read_to_string(&p).unwrap();
        assert!(text.len() as u64 <= MAX_BYTES);
        assert!(text.lines().last().unwrap().starts_with("[199] 199 "));
        // Every remaining line is whole.
        assert!(text.lines().all(|l| l.starts_with('[') && l.ends_with('x')));
        let _ = std::fs::remove_dir_all(p.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn unwritable_location_is_an_error_not_a_panic() {
        // A file where the directory should be.
        let p = temp("err");
        std::fs::create_dir_all(p.parent().unwrap().parent().unwrap()).unwrap();
        std::fs::write(p.parent().unwrap(), b"not a dir").unwrap();
        assert!(append_to(&p, "x", 1).is_err());
        let _ = std::fs::remove_dir_all(p.parent().unwrap().parent().unwrap());
    }
}
