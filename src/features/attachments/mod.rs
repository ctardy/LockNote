// Attachments — Encrypted file attachments
//
// Stores binary files (images, PDF, documents) inside the encrypted payload,
// alongside tabs, vault, and snapshots blocks.
//
// Format:
//   [LOCKNOTE_ATTACHMENTS]
//   [FILE:rapport.pdf|size:245760|sha256:abc123...]
//   <base64-encoded content>
//   [/FILE]
//   [FILE:photo.jpg|size:102400|sha256:def456...]
//   <base64-encoded content>
//   [/FILE]
//   [/LOCKNOTE_ATTACHMENTS]

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use sha2::{Sha256, Digest};

/// Default maximum size per attachment (10 MB).
pub const DEFAULT_MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// Default maximum total size for all attachments (50 MB).
pub const DEFAULT_MAX_TOTAL_SIZE: u64 = 50 * 1024 * 1024;

/// Ceiling on the base64 accumulator inside a single `[FILE:…]` block.
/// Guards against a vault crafted to omit `[/FILE]` and blow up the parser's
/// working memory. Must accommodate one legitimate attachment expanded to
/// base64 (≈4/3× the raw size), plus a generous margin. Any `[FILE:…]` block
/// that exceeds this is discarded and parsing stops.
const MAX_ATTACH_B64_ACCUM: usize = 80 * 1024 * 1024; // 80 MiB of base64 text

use crate::features::markers::{
    escape_content, find_loose_marker_line, find_marker_line, unescape_content, ATTACH_FOOTER, ATTACH_HEADER, FILE_END, FILE_PREFIX,
};

/// A single file attachment.
#[derive(Debug, Clone, PartialEq)]
pub struct Attachment {
    /// Original file name (e.g. "rapport.pdf").
    pub name: String,
    /// Raw binary content of the file.
    pub data: Vec<u8>,
    /// SHA-256 hash of `data`, hex-encoded.
    pub sha256: String,
}

impl Attachment {
    /// Create a new attachment from raw bytes, computing the SHA-256 hash.
    pub fn new(name: &str, data: Vec<u8>) -> Self {
        let sha256 = compute_sha256(&data);
        Attachment {
            name: name.to_string(),
            data,
            sha256,
        }
    }

    /// Size of the raw data in bytes.
    pub fn size(&self) -> u64 {
        self.data.len() as u64
    }

    /// Verify that the stored SHA-256 matches the actual data.
    pub fn verify(&self) -> bool {
        compute_sha256(&self.data) == self.sha256
    }

    /// Wipe the raw attachment bytes in place. `name` and `sha256` stay —
    /// they're not secret. This is called at auto-lock.
    pub fn wipe(&mut self) {
        use zeroize::Zeroize;
        self.data.zeroize();
    }
}

/// Compute hex-encoded SHA-256 of data.
fn compute_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    hex_encode(&result)
}

/// Encode bytes as lowercase hex string.
fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// Format a byte size for human-readable display.
pub fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// Total size of all attachments in bytes.
pub fn total_size(attachments: &[Attachment]) -> u64 {
    attachments.iter().map(|a| a.size()).sum()
}

/// Where the parser sees the attachments block start in `input`: the first
/// line that is the header and nothing else, never text inside a line nor an
/// escaped line of a tab or of a vault value (issue 050).
pub fn header_line(input: &str) -> Option<usize> {
    find_marker_line(input, ATTACH_HEADER, 0)
}

/// `header_line` with a lone `\r` counted as a line bound too: where the
/// parser sees the attachments header, or could once the reading chain has dropped
/// line breaks around it (`markers::find_loose_marker_line`). For the
/// decision made when writing (`payload::compose`), not for reading.
pub fn loose_header_line(input: &str) -> Option<usize> {
    find_loose_marker_line(input, ATTACH_HEADER, 0)
}

/// Parse attachments block from input. Returns (attachments, remaining_data).
/// The attachments block is expected at the end of the input; everything before
/// it is returned as remaining data.
pub fn parse(input: &str) -> (Vec<Attachment>, String) {
    if input.is_empty() {
        return (Vec::new(), String::new());
    }

    let attach_start = match header_line(input) {
        Some(pos) => pos,
        None => return (Vec::new(), input.to_string()),
    };

    let before = &input[..attach_start];
    let before = before.trim_end_matches('\n').trim_end_matches('\r');

    let after_header = attach_start + ATTACH_HEADER.len();
    let block = &input[after_header..];

    let mut attachments: Vec<Attachment> = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_size: u64 = 0;
    let mut current_sha256 = String::new();
    let mut current_b64 = String::new();

    for line in block.split('\n') {
        let trimmed = line.trim_start();

        // Footer — stop parsing
        if trimmed == ATTACH_FOOTER {
            break;
        }

        // File header: [FILE:name|size:N|sha256:hex]
        if trimmed.starts_with(FILE_PREFIX) && !line.starts_with('\x00') {
            if let Some(end) = trimmed.find(']') {
                let inner = &trimmed[FILE_PREFIX.len()..end];

                // Save previous attachment
                if let Some(name) = current_name.take() {
                    let b64_clean = unescape_content(&current_b64);
                    if let Some(att) = decode_attachment(&name, current_size, &current_sha256, &b64_clean) {
                        attachments.push(att);
                    }
                    current_b64.clear();
                }

                // Parse "name|size:N|sha256:hex"
                if let Some((name, size, sha256)) = parse_file_header(inner) {
                    current_name = Some(name);
                    current_size = size;
                    current_sha256 = sha256;
                }
                continue;
            }
        }

        // File end marker
        if trimmed == FILE_END && !line.starts_with('\x00') {
            if let Some(name) = current_name.take() {
                let b64_clean = unescape_content(&current_b64);
                if let Some(att) = decode_attachment(&name, current_size, &current_sha256, &b64_clean) {
                    attachments.push(att);
                }
                current_b64.clear();
                current_sha256.clear();
                current_size = 0;
            }
            continue;
        }

        // Accumulate base64 lines
        if current_name.is_some() {
            if !current_b64.is_empty() {
                current_b64.push('\n');
            }
            current_b64.push_str(line);
            // Cap accumulation so a malformed block without [/FILE] can't
            // grow unbounded. Base64 expands ~4/3×; the cap here is well
            // above any legitimate single attachment (they're individually
            // limited by max_attachment_size_mb at UI attach time).
            if current_b64.len() > MAX_ATTACH_B64_ACCUM {
                // Abort this file block, discard its partial content, and
                // stop parsing subsequent lines as attachments. The outer
                // payload was HMAC-validated, so this only fires on a
                // malicious author-controlled vault, not a network tamper.
                current_name = None;
                current_b64.clear();
                break;
            }
        }
    }

    // Handle unclosed file block
    if let Some(name) = current_name.take() {
        let b64_clean = unescape_content(&current_b64);
        if let Some(att) = decode_attachment(&name, current_size, &current_sha256, &b64_clean) {
            attachments.push(att);
        }
    }

    (attachments, before.to_string())
}

/// Parse the file header inner string: "name|size:N|sha256:hex"
fn parse_file_header(inner: &str) -> Option<(String, u64, String)> {
    let parts: Vec<&str> = inner.splitn(3, '|').collect();
    if parts.len() != 3 {
        return None;
    }

    let name = parts[0].to_string();

    let size = parts[1].strip_prefix("size:")?;
    let size: u64 = size.parse().ok()?;

    let sha256 = parts[2].strip_prefix("sha256:")?;
    let sha256 = sha256.to_string();

    Some((name, size, sha256))
}

/// Decode base64 content and build an Attachment, verifying SHA-256.
fn decode_attachment(name: &str, expected_size: u64, expected_sha256: &str, b64: &str) -> Option<Attachment> {
    // Strip whitespace from base64 (line breaks within the encoded data)
    let clean: String = b64.chars().filter(|c| !c.is_whitespace()).collect();
    let data = BASE64.decode(&clean).ok()?;

    // Verify size
    if data.len() as u64 != expected_size {
        return None;
    }

    let sha256 = compute_sha256(&data);

    // Verify hash
    if sha256 != expected_sha256 {
        return None;
    }

    Some(Attachment {
        name: name.to_string(),
        data,
        sha256,
    })
}

/// Serialize attachments into the attachments block format.
pub fn serialize(attachments: &[Attachment]) -> String {
    if attachments.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    out.push_str(ATTACH_HEADER);
    out.push('\n');

    for att in attachments {
        // File header
        out.push_str(FILE_PREFIX);
        out.push_str(&att.name);
        out.push_str(&format!("|size:{}|sha256:{}", att.data.len(), att.sha256));
        out.push_str("]\n");

        // Base64 content, split into 76-char lines for readability
        let encoded = BASE64.encode(&att.data);
        let b64_escaped = escape_content(&encoded);
        for chunk in b64_escaped.as_bytes().chunks(76) {
            out.push_str(&String::from_utf8_lossy(chunk));
            out.push('\n');
        }

        out.push_str(FILE_END);
        out.push('\n');
    }

    out.push_str(ATTACH_FOOTER);
    out
}

/// Combine data and attachments into a single string.
/// `data` is the tabs+vault+history portion; attachments block is appended after it.
pub fn combine(data: &str, attachments: &[Attachment]) -> String {
    let attach_block = serialize(attachments);
    if attach_block.is_empty() {
        return data.to_string();
    }
    if data.is_empty() {
        return attach_block;
    }
    format!("{}\n{}", data, attach_block)
}

/// Check if a file can be added within size limits.
pub fn check_limits(
    existing: &[Attachment],
    new_size: u64,
    max_file_size: u64,
    max_total_size: u64,
) -> Result<(), AttachmentError> {
    if new_size > max_file_size {
        return Err(AttachmentError::FileTooLarge {
            size: new_size,
            max: max_file_size,
        });
    }
    let current_total = total_size(existing);
    if current_total + new_size > max_total_size {
        return Err(AttachmentError::TotalTooLarge {
            total: current_total + new_size,
            max: max_total_size,
        });
    }
    Ok(())
}

/// Errors related to attachment operations.
#[derive(Debug, Clone, PartialEq)]
pub enum AttachmentError {
    FileTooLarge { size: u64, max: u64 },
    TotalTooLarge { total: u64, max: u64 },
}

impl std::fmt::Display for AttachmentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AttachmentError::FileTooLarge { size, max } => {
                write!(f, "File too large: {} (max {})", format_size(*size), format_size(*max))
            }
            AttachmentError::TotalTooLarge { total, max } => {
                write!(f, "Total attachments too large: {} (max {})", format_size(*total), format_size(*max))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_input() {
        let (attachments, remaining) = parse("");
        assert!(attachments.is_empty());
        assert_eq!(remaining, "");
    }

    #[test]
    fn test_no_attachments() {
        let (attachments, remaining) = parse("Just tab data here");
        assert!(attachments.is_empty());
        assert_eq!(remaining, "Just tab data here");
    }

    #[test]
    fn test_single_attachment() {
        let data = b"Hello, world!";
        let att = Attachment::new("test.txt", data.to_vec());
        let serialized = serialize(&[att.clone()]);
        let (parsed, remaining) = parse(&serialized);

        assert_eq!(remaining, "");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "test.txt");
        assert_eq!(parsed[0].data, data.to_vec());
        assert_eq!(parsed[0].sha256, att.sha256);
    }

    #[test]
    fn test_multiple_attachments() {
        let att1 = Attachment::new("file1.txt", b"Content one".to_vec());
        let att2 = Attachment::new("file2.pdf", b"Content two".to_vec());
        let serialized = serialize(&[att1.clone(), att2.clone()]);
        let (parsed, _) = parse(&serialized);

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].name, "file1.txt");
        assert_eq!(parsed[0].data, att1.data);
        assert_eq!(parsed[1].name, "file2.pdf");
        assert_eq!(parsed[1].data, att2.data);
    }

    #[test]
    fn test_roundtrip_binary_data() {
        // Binary data with all byte values
        let data: Vec<u8> = (0..=255).collect();
        let att = Attachment::new("binary.bin", data.clone());
        let serialized = serialize(&[att]);
        let (parsed, _) = parse(&serialized);

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].data, data);
    }

    #[test]
    fn test_roundtrip_large_data() {
        // Larger data that spans multiple base64 lines
        let data: Vec<u8> = (0..1000).map(|i| (i % 256) as u8).collect();
        let att = Attachment::new("large.bin", data.clone());
        let serialized = serialize(&[att]);
        let (parsed, _) = parse(&serialized);

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].data, data);
    }

    #[test]
    fn test_data_before_attachments() {
        let tab_data = "[LOCKNOTE_TABS]\n[TAB:Note 1]\nHello\n[/LOCKNOTE_TABS]";
        let att = Attachment::new("doc.pdf", b"PDF content".to_vec());
        let combined = combine(tab_data, &[att.clone()]);

        assert!(combined.starts_with("[LOCKNOTE_TABS]"));
        assert!(combined.contains("[LOCKNOTE_ATTACHMENTS]"));

        let (parsed, parsed_data) = parse(&combined);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "doc.pdf");
        assert_eq!(parsed_data, tab_data);
    }

    #[test]
    fn test_combine_empty_attachments() {
        let data = "tab content";
        let combined = combine(data, &[]);
        assert_eq!(combined, "tab content");
    }

    #[test]
    fn test_combine_empty_data() {
        let att = Attachment::new("test.txt", b"content".to_vec());
        let combined = combine("", &[att]);
        assert!(combined.starts_with("[LOCKNOTE_ATTACHMENTS]"));
    }

    #[test]
    fn test_serialize_empty() {
        assert_eq!(serialize(&[]), "");
    }

    #[test]
    fn test_sha256_verification() {
        let att = Attachment::new("test.txt", b"hello".to_vec());
        assert!(att.verify());

        let mut bad = att.clone();
        bad.data = b"tampered".to_vec();
        assert!(!bad.verify());
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(1048576), "1.0 MB");
        assert_eq!(format_size(10485760), "10.0 MB");
    }

    #[test]
    fn test_total_size() {
        let atts = vec![
            Attachment::new("a.txt", vec![0; 100]),
            Attachment::new("b.txt", vec![0; 200]),
        ];
        assert_eq!(total_size(&atts), 300);
    }

    #[test]
    fn test_check_limits_ok() {
        let existing = vec![Attachment::new("a.txt", vec![0; 1000])];
        assert!(check_limits(&existing, 5000, DEFAULT_MAX_FILE_SIZE, DEFAULT_MAX_TOTAL_SIZE).is_ok());
    }

    #[test]
    fn test_check_limits_file_too_large() {
        let result = check_limits(&[], DEFAULT_MAX_FILE_SIZE + 1, DEFAULT_MAX_FILE_SIZE, DEFAULT_MAX_TOTAL_SIZE);
        assert!(matches!(result, Err(AttachmentError::FileTooLarge { .. })));
    }

    #[test]
    fn test_check_limits_total_too_large() {
        let existing = vec![Attachment::new("big.bin", vec![0; DEFAULT_MAX_TOTAL_SIZE as usize - 100])];
        let result = check_limits(&existing, 200, DEFAULT_MAX_FILE_SIZE, DEFAULT_MAX_TOTAL_SIZE);
        assert!(matches!(result, Err(AttachmentError::TotalTooLarge { .. })));
    }

    #[test]
    fn test_attachment_new_computes_hash() {
        let att = Attachment::new("test.txt", b"test data".to_vec());
        assert!(!att.sha256.is_empty());
        assert_eq!(att.sha256.len(), 64); // hex-encoded SHA-256 = 64 chars
        assert!(att.verify());
    }

    #[test]
    fn test_escaping_markers_in_filename() {
        // File with a name that looks like a marker (unlikely but defensive)
        let att = Attachment::new("normal.txt", b"data".to_vec());
        let serialized = serialize(&[att.clone()]);
        let (parsed, _) = parse(&serialized);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].data, att.data);
    }

    #[test]
    fn test_empty_file() {
        let att = Attachment::new("empty.txt", vec![]);
        let serialized = serialize(&[att.clone()]);
        let (parsed, _) = parse(&serialized);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "empty.txt");
        assert_eq!(parsed[0].data, Vec::<u8>::new());
        assert!(parsed[0].verify());
    }

    #[test]
    fn test_error_display() {
        let e = AttachmentError::FileTooLarge { size: 15 * 1024 * 1024, max: 10 * 1024 * 1024 };
        let msg = format!("{}", e);
        assert!(msg.contains("15.0 MB"));
        assert!(msg.contains("10.0 MB"));
    }
}
