// Snapshots — legacy version history block (read only)
//
// Snapshots are no longer created, shown or written: the safety backups of
// `features::auto_backup` replaced them. What stays is the reader, because a file
// written by an older LockNote still holds the block, and an unparsed block
// would be glued into the last tab. `parse` strips it; the data is dropped at
// the next save. `serialize` and `combine` are kept for tests only, to build
// such old files.
//
// Format of the block:
//   [LOCKNOTE_HISTORY]
//   [SNAP:1712930422]
//   content
//   [/SNAP]
//   [SNAP:1712930500]
//   content
//   [/SNAP]
//   [/LOCKNOTE_HISTORY]

/// Ceiling on a single snapshot's content accumulator. Protects the parser
/// against a malformed `[SNAP:…]` block missing its `[/SNAP]` terminator
/// (which would otherwise slurp everything up to the next header).
const MAX_SNAPSHOT_CONTENT: usize = 16 * 1024 * 1024; // 16 MiB per snapshot

#[cfg(test)]
use crate::features::markers::escape_content;
use crate::crypto::wiping::WipingString;
use crate::features::markers::{find_loose_marker_line, find_marker_line, unescape_content, HISTORY_FOOTER, HISTORY_HEADER, SNAP_END, SNAP_PREFIX};
use zeroize::Zeroizing;

/// A single snapshot of the note content at a point in time.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    /// Unix timestamp (seconds since epoch).
    pub timestamp: u64,
    /// Optional user-given label.
    pub name: String,
    /// Full tab data at that point.
    pub content: String,
}

impl Snapshot {
    #[cfg(test)]
    pub fn new(timestamp: u64, content: &str) -> Self {
        Snapshot {
            timestamp,
            name: String::new(),
            content: content.to_string(),
        }
    }

    pub fn with_name(timestamp: u64, name: &str, content: &str) -> Self {
        Snapshot {
            timestamp,
            name: name.to_string(),
            content: content.to_string(),
        }
    }

    /// Wipe the plaintext `content` of this snapshot. The timestamp/name are
    /// not secrets; `content` is the full tab data captured at the time and
    /// must not survive in RAM past an auto-lock.
    pub fn wipe(&mut self) {
        use zeroize::Zeroize;
        self.content.zeroize();
    }
}

/// `s` without one trailing `\n` (and the `\r` before it): a slice, not a
/// copy of the text (issue #42).
fn strip_trailing_newline(s: &str) -> &str {
    match s.strip_suffix('\n') {
        Some(rest) => rest.strip_suffix('\r').unwrap_or(rest),
        None => s,
    }
}

/// Where the parser sees the history block start in `input`: the first line
/// that is the header and nothing else, never text inside a line nor an
/// escaped line of a tab (issue 050).
pub fn header_line(input: &str) -> Option<usize> {
    find_marker_line(input, HISTORY_HEADER, 0)
}

/// `header_line` with a lone `\r` counted as a line bound too: where the
/// parser sees the history header, or could once the reading chain has dropped
/// line breaks around it (`markers::find_loose_marker_line`). For the
/// decision made when writing (`payload::compose`), not for reading.
pub fn loose_header_line(input: &str) -> Option<usize> {
    find_loose_marker_line(input, HISTORY_HEADER, 0)
}

/// Parse history block from input. Returns (snapshots, remaining_data).
/// The history block is expected at the end of the input; everything before it
/// is returned as remaining data (tabs + vault).
pub fn parse(input: &str) -> (Vec<Snapshot>, String) {
    if input.is_empty() {
        return (Vec::new(), String::new());
    }

    let history_start = match header_line(input) {
        Some(pos) => pos,
        None => return (Vec::new(), input.to_string()),
    };

    let before = &input[..history_start];
    let before = before.trim_end_matches('\n').trim_end_matches('\r');

    let after_header = history_start + HISTORY_HEADER.len();
    let block = &input[after_header..];

    let mut snapshots: Vec<Snapshot> = Vec::new();
    let mut current_ts: Option<u64> = None;
    let mut current_name = String::new();
    // Grows without leaving copies; the text read is wiped once in its
    // snapshot (issue #42).
    let mut current_content = WipingString::new();

    for line in block.split('\n') {
        let trimmed = line.trim_start();

        // Check for real (unescaped) history footer
        if trimmed == HISTORY_FOOTER {
            break;
        }

        // Check for [SNAP:timestamp] or [SNAP:timestamp:name]
        if trimmed.starts_with(SNAP_PREFIX) && !line.starts_with('\x00') {
            if let Some(end) = trimmed.find(']') {
                let inner = &trimmed[SNAP_PREFIX.len()..end];

                // Save previous snapshot
                if let Some(ts) = current_ts.take() {
                    let content = Zeroizing::new(unescape_content(strip_trailing_newline(current_content.as_str())));
                    snapshots.push(Snapshot::with_name(ts, &current_name, &content));
                    current_content.clear();
                    current_name.clear();
                }

                // Parse "timestamp" or "timestamp:name"
                if let Some(colon) = inner.find(':') {
                    if let Ok(ts) = inner[..colon].parse::<u64>() {
                        current_ts = Some(ts);
                        current_name = inner[colon + 1..].to_string();
                    }
                } else if let Ok(ts) = inner.parse::<u64>() {
                    current_ts = Some(ts);
                }
                continue;
            }
        }

        // Check for [/SNAP]
        if trimmed == SNAP_END && !line.starts_with('\x00') {
            if let Some(ts) = current_ts.take() {
                let content = Zeroizing::new(unescape_content(strip_trailing_newline(current_content.as_str())));
                snapshots.push(Snapshot::with_name(ts, &current_name, &content));
                current_content.clear();
                current_name.clear();
            }
            continue;
        }

        if current_ts.is_some() {
            if !current_content.is_empty() {
                current_content.push('\n');
            }
            current_content.push_str(line);
            if current_content.len() > MAX_SNAPSHOT_CONTENT {
                // Malformed / malicious snapshot — abandon it and stop
                // parsing further. The outer payload is HMAC-verified, so
                // this only protects against a vault whose author is the
                // attacker.
                current_ts = None;
                current_content.clear();
                current_name.clear();
                break;
            }
        }
    }

    // Save any unclosed snapshot
    if let Some(ts) = current_ts.take() {
        let content = Zeroizing::new(unescape_content(strip_trailing_newline(current_content.as_str())));
        snapshots.push(Snapshot::with_name(ts, &current_name, &content));
    }

    (snapshots, before.to_string())
}

/// Serialize snapshots into the history format.
#[cfg(test)]
pub fn serialize(snapshots: &[Snapshot]) -> String {
    if snapshots.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    out.push_str(HISTORY_HEADER);
    out.push('\n');

    for snap in snapshots {
        out.push_str(SNAP_PREFIX);
        out.push_str(&snap.timestamp.to_string());
        if !snap.name.is_empty() {
            out.push(':');
            out.push_str(&snap.name);
        }
        out.push_str("]\n");
        out.push_str(&escape_content(&snap.content));
        if !snap.content.is_empty() && !snap.content.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(SNAP_END);
        out.push('\n');
    }

    out.push_str(HISTORY_FOOTER);
    out
}

/// Combine data and snapshots into a single string.
/// `data` is the tabs+vault portion; history block is appended after it.
#[cfg(test)]
pub fn combine(data: &str, snapshots: &[Snapshot]) -> String {
    let history_block = serialize(snapshots);
    if history_block.is_empty() {
        return data.to_string();
    }
    if data.is_empty() {
        return history_block;
    }
    format!("{}\n{}", data, history_block)
}

/// Get the current Unix timestamp.
pub fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Format a Unix timestamp as "YYYY-MM-DD HH:MM:SS" (local time, manual calc).
pub fn format_timestamp(ts: u64) -> String {
    // We avoid pulling in chrono; do a simple UTC formatting.
    // For a Windows desktop app we can use the system's local offset,
    // but for simplicity we show UTC with a note.
    let secs = ts;
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Calculate date from days since epoch (1970-01-01)
    let (year, month, day) = days_to_date(days as i64);

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        year, month, day, hours, minutes, seconds
    )
}

/// Convert days since Unix epoch to (year, month, day).
fn days_to_date(days: i64) -> (i64, u32, u32) {
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32; // day of era [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_input() {
        let (snapshots, remaining) = parse("");
        assert!(snapshots.is_empty());
        assert_eq!(remaining, "");
    }

    #[test]
    fn test_no_history() {
        let (snapshots, remaining) = parse("Just tab data here");
        assert!(snapshots.is_empty());
        assert_eq!(remaining, "Just tab data here");
    }

    #[test]
    fn test_single_snapshot() {
        let input = "[LOCKNOTE_HISTORY]\n[SNAP:1712930422]\nHello world\n[/SNAP]\n[/LOCKNOTE_HISTORY]";
        let (snapshots, remaining) = parse(input);
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].timestamp, 1712930422);
        assert_eq!(snapshots[0].content, "Hello world");
        assert_eq!(remaining, "");
    }

    #[test]
    fn test_multiple_snapshots() {
        let input = "[LOCKNOTE_HISTORY]\n[SNAP:100]\nFirst\n[/SNAP]\n[SNAP:200]\nSecond\n[/SNAP]\n[/LOCKNOTE_HISTORY]";
        let (snapshots, _) = parse(input);
        assert_eq!(snapshots.len(), 2);
        assert_eq!(snapshots[0].timestamp, 100);
        assert_eq!(snapshots[0].content, "First");
        assert_eq!(snapshots[1].timestamp, 200);
        assert_eq!(snapshots[1].content, "Second");
    }

    #[test]
    fn test_roundtrip() {
        let snaps = vec![
            Snapshot::new(1000, "First version"),
            Snapshot::new(2000, "Second version\nwith multiple lines"),
            Snapshot::new(3000, "Third"),
        ];
        let serialized = serialize(&snaps);
        let (parsed, remaining) = parse(&serialized);
        assert_eq!(remaining, "");
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0], snaps[0]);
        assert_eq!(parsed[1], snaps[1]);
        assert_eq!(parsed[2], snaps[2]);
    }

    #[test]
    fn test_roundtrip_with_data_before() {
        let tab_data = "[LOCKNOTE_TABS]\n[TAB:Note 1]\nHello\n[/LOCKNOTE_TABS]";
        let snaps = vec![Snapshot::new(1000, "Old content")];
        let combined = combine(tab_data, &snaps);

        assert!(combined.starts_with("[LOCKNOTE_TABS]"));
        assert!(combined.contains("[LOCKNOTE_HISTORY]"));

        let (parsed_snaps, parsed_data) = parse(&combined);
        assert_eq!(parsed_snaps.len(), 1);
        assert_eq!(parsed_snaps[0].content, "Old content");
        assert_eq!(parsed_data, tab_data);
    }

    #[test]
    fn test_empty_snapshots_serialize() {
        assert_eq!(serialize(&[]), "");
    }

    #[test]
    fn test_combine_empty_snapshots() {
        let data = "tab content";
        let combined = combine(data, &[]);
        assert_eq!(combined, "tab content");
    }

    #[test]
    fn test_combine_empty_data() {
        let snaps = vec![Snapshot::new(100, "content")];
        let combined = combine("", &snaps);
        assert!(combined.starts_with("[LOCKNOTE_HISTORY]"));
    }

    #[test]
    fn test_escaping_markers_in_content() {
        let snap = Snapshot::new(100, "[SNAP:fake]\n[/SNAP]\n[LOCKNOTE_HISTORY]\n[/LOCKNOTE_HISTORY]");
        let serialized = serialize(&[snap.clone()]);
        let (parsed, _) = parse(&serialized);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].content, snap.content);
    }

    #[test]
    fn test_multiline_content() {
        let snap = Snapshot::new(100, "Line 1\nLine 2\nLine 3");
        let serialized = serialize(&[snap.clone()]);
        let (parsed, _) = parse(&serialized);
        assert_eq!(parsed[0].content, snap.content);
    }

    #[test]
    fn test_format_timestamp() {
        // 2024-04-12 15:00:22 UTC = 1712934022
        let formatted = format_timestamp(1712934022);
        assert_eq!(formatted, "2024-04-12 15:00:22");
    }

    #[test]
    fn test_format_timestamp_epoch() {
        assert_eq!(format_timestamp(0), "1970-01-01 00:00:00");
    }

    #[test]
    fn test_data_before_history() {
        let input = "Some data\nMore data\n[LOCKNOTE_HISTORY]\n[SNAP:500]\nold\n[/SNAP]\n[/LOCKNOTE_HISTORY]";
        let (snaps, data) = parse(input);
        assert_eq!(data, "Some data\nMore data");
        assert_eq!(snaps.len(), 1);
        assert_eq!(snaps[0].timestamp, 500);
        assert_eq!(snaps[0].content, "old");
    }

    #[test]
    fn test_empty_snapshot_content() {
        let snap = Snapshot::new(100, "");
        let serialized = serialize(&[snap]);
        let (parsed, _) = parse(&serialized);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].content, "");
    }
}
