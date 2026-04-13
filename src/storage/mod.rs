// Storage module — Binary marker, read/write encrypted payload, .tmp management
//
// The LockNote binary format appends encrypted data after the PE executable:
//   [exe binary] [16-byte XOR-obfuscated marker] [encrypted payload]
//
// A .tmp staging file in %LOCALAPPDATA%\LockNote\ is used for atomic writes,
// since the running .exe cannot overwrite itself.

use sha2::{Sha256, Digest};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// XOR key used to obfuscate the marker in the binary.
const XOR_KEY: u8 = 0xAA;

/// Minimum valid payload size: salt(16) + iv(16) + hmac(32) + one AES block(16).
const MIN_PAYLOAD_SIZE: usize = 80;

/// Marker length in bytes.
const MARKER_LEN: usize = 16;

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
fn get_marker() -> [u8; MARKER_LEN] {
    let mut marker = [0u8; MARKER_LEN];
    for i in 0..MARKER_LEN {
        marker[i] = MARKER_XORED[i] ^ XOR_KEY;
    }
    marker
}

/// Returns a copy of the marker bytes (used by Updater for data migration).
pub fn get_marker_for_update() -> Vec<u8> {
    get_marker().to_vec()
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

/// Compute the .tmp staging file path in %LOCALAPPDATA%\LockNote\.
///
/// The filename is the first 8 bytes of SHA256(exePath.ToUpperInvariant()) as
/// lowercase hex, ensuring case-insensitive uniqueness across exe locations.
pub fn get_tmp_path(exe_path: &Path) -> PathBuf {
    let local_app_data = std::env::var("LOCALAPPDATA")
        .expect("LOCALAPPDATA environment variable not set");

    let dir = PathBuf::from(&local_app_data).join("LockNote");

    // Convert path to uppercase string for case-insensitive hashing.
    // Use the OS string representation (backslashes on Windows).
    let path_str = exe_path.to_string_lossy().to_uppercase();

    let mut hasher = Sha256::new();
    hasher.update(path_str.as_bytes());
    let hash = hasher.finalize();

    // First 8 bytes as lowercase hex
    let name: String = hash[..8].iter()
        .map(|b| format!("{:02x}", b))
        .collect();

    dir.join(format!("{}.tmp", name))
}

/// Read encrypted payload from either the .tmp staging file or the .exe itself.
///
/// Priority: .tmp first (may contain a pending swap), then .exe.
/// Returns the payload bytes after the marker, or None if no valid data found.
pub fn read_data(exe_path: &Path) -> Option<Vec<u8>> {
    let tmp_path = get_tmp_path(exe_path);

    // Try .tmp first (pending swap data)
    if tmp_path.exists() {
        if let Some(payload) = read_payload_from_file(&tmp_path) {
            return Some(payload);
        }
    }

    // Fall back to exe
    if exe_path.exists() {
        return read_payload_from_file(exe_path);
    }

    None
}

/// Extract the encrypted payload from a file containing the marker.
fn read_payload_from_file(path: &Path) -> Option<Vec<u8>> {
    let data = fs::read(path).ok()?;
    let pos = find_marker(&data)?;

    let data_start = pos + MARKER_LEN;
    if data_start >= data.len() {
        return None;
    }

    let payload_len = data.len() - data_start;
    if payload_len < MIN_PAYLOAD_SIZE {
        return None;
    }

    Some(data[data_start..].to_vec())
}

/// Write the exe binary + marker + encrypted payload to the .tmp staging file.
///
/// Reads the current exe to extract only the clean binary (before any existing
/// marker), then writes: [clean exe] [marker] [encrypted_payload] to .tmp.
pub fn write_data(exe_path: &Path, encrypted: &[u8]) -> io::Result<()> {
    let exe_data = fs::read(exe_path)?;

    // Find the clean exe portion (everything before the marker)
    let clean_len = match find_marker(&exe_data) {
        Some(pos) => pos,
        None => exe_data.len(),
    };

    let tmp_path = get_tmp_path(exe_path);

    // Ensure the LockNote directory exists
    if let Some(parent) = tmp_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let marker = get_marker();

    // Build the complete file content
    let mut output = Vec::with_capacity(clean_len + MARKER_LEN + encrypted.len());
    output.extend_from_slice(&exe_data[..clean_len]);
    output.extend_from_slice(&marker);
    output.extend_from_slice(encrypted);

    fs::write(&tmp_path, &output)?;
    Ok(())
}

/// Delete .tmp files older than 1 minute in %LOCALAPPDATA%\LockNote\,
/// skipping the current exe's .tmp (which may contain valid pending data).
pub fn cleanup_stale_tmp_files() {
    let exe_path = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return,
    };

    let current_tmp = get_tmp_path(&exe_path);

    let local_app_data = match std::env::var("LOCALAPPDATA") {
        Ok(v) => v,
        Err(_) => return,
    };

    let dir = PathBuf::from(&local_app_data).join("LockNote");
    if !dir.exists() {
        return;
    }

    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    let one_minute = std::time::Duration::from_secs(60);

    for entry in entries.flatten() {
        let path = entry.path();

        // Only process .tmp files
        if path.extension().and_then(|e| e.to_str()) != Some("tmp") {
            continue;
        }

        // Skip current exe's tmp file
        if path == current_tmp {
            continue;
        }

        // Check age: delete if older than 1 minute
        let metadata = match fs::metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let modified = match metadata.modified() {
            Ok(t) => t,
            Err(_) => continue,
        };

        let age = SystemTime::now().duration_since(modified).unwrap_or_default();
        if age > one_minute {
            let _ = fs::remove_file(&path);
        }
    }
}

/// Build a cmd.exe command that waits for the process to exit, then moves
/// the .tmp file over the .exe (atomic swap on Windows).
///
/// The ping command provides a ~2-second delay for the process to fully exit
/// before the move overwrites the locked executable.
pub fn atomic_swap_command(tmp_path: &Path, exe_path: &Path) -> std::process::Command {
    let tmp_str = tmp_path.to_string_lossy();
    let exe_str = exe_path.to_string_lossy();

    // Use raw_arg to avoid MSVC argument encoding which breaks cmd.exe
    // (cmd.exe does not understand \" as escaped quotes)
    let raw = format!(
        "/c ping -n 3 127.0.0.1 >nul & move /y \"{}\" \"{}\"",
        tmp_str, exe_str
    );

    let mut cmd = std::process::Command::new("cmd.exe");
    // Hide the console window
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd.raw_arg(&raw);
    }
    cmd
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

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

    // ── read_data tests ──

    #[test]
    fn read_data_no_marker_returns_none() {
        let dir = std::env::temp_dir().join("locknote_test_no_marker");
        let _ = fs::create_dir_all(&dir);
        let file = dir.join("test_no_marker.exe");
        fs::write(&file, &[0u8; 500]).unwrap();

        let result = read_payload_from_file(&file);
        assert!(result.is_none());

        let _ = fs::remove_file(&file);
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn read_data_payload_too_small_returns_none() {
        let dir = std::env::temp_dir().join("locknote_test_small_payload");
        let _ = fs::create_dir_all(&dir);
        let file = dir.join("test_small.exe");

        let marker = get_marker();
        let mut data = vec![0u8; 100]; // exe stub
        data.extend_from_slice(&marker);
        data.extend_from_slice(&[0xAB; 50]); // less than MIN_PAYLOAD_SIZE (80)
        fs::write(&file, &data).unwrap();

        let result = read_payload_from_file(&file);
        assert!(result.is_none());

        let _ = fs::remove_file(&file);
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn read_data_nonexistent_file_returns_none() {
        let result = read_payload_from_file(Path::new("C:\\nonexistent\\file.exe"));
        assert!(result.is_none());
    }

    // ── Round-trip write/read test ──

    #[test]
    fn roundtrip_write_then_read() {
        let dir = std::env::temp_dir().join("locknote_test_roundtrip");
        let _ = fs::create_dir_all(&dir);
        let exe_file = dir.join("roundtrip.exe");

        // Create a fake exe (no marker)
        let exe_stub = vec![0x4Du8, 0x5A, 0x90, 0x00]; // MZ header stub
        fs::write(&exe_file, &exe_stub).unwrap();

        // Fake encrypted payload (>= 80 bytes)
        let payload: Vec<u8> = (0..100).map(|i| (i as u8).wrapping_mul(7)).collect();

        // Write
        write_data(&exe_file, &payload).unwrap();

        // The tmp file should now exist
        let tmp_path = get_tmp_path(&exe_file);
        assert!(tmp_path.exists());

        // Read from tmp
        let recovered = read_payload_from_file(&tmp_path).unwrap();
        assert_eq!(recovered, payload);

        // read_data should find it via tmp
        let via_read_data = read_data(&exe_file).unwrap();
        assert_eq!(via_read_data, payload);

        // Verify the tmp file structure: [exe_stub][marker][payload]
        let raw = fs::read(&tmp_path).unwrap();
        assert_eq!(&raw[..exe_stub.len()], &exe_stub);
        assert_eq!(&raw[exe_stub.len()..exe_stub.len() + MARKER_LEN], &get_marker());
        assert_eq!(&raw[exe_stub.len() + MARKER_LEN..], &payload[..]);

        // Cleanup
        let _ = fs::remove_file(&tmp_path);
        let _ = fs::remove_file(&exe_file);
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn write_strips_old_marker_and_payload() {
        let dir = std::env::temp_dir().join("locknote_test_strip_old");
        let _ = fs::create_dir_all(&dir);
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
        let new_payload: Vec<u8> = (0..120).collect();
        write_data(&exe_file, &new_payload).unwrap();

        // Read back: should get new payload, not old
        let tmp_path = get_tmp_path(&exe_file);
        let raw = fs::read(&tmp_path).unwrap();

        // Clean exe portion should be exactly the stub (200 bytes)
        assert_eq!(raw.len(), 200 + MARKER_LEN + new_payload.len());
        assert_eq!(&raw[..200], &exe_stub[..]);

        let recovered = read_payload_from_file(&tmp_path).unwrap();
        assert_eq!(recovered, new_payload);

        // Cleanup
        let _ = fs::remove_file(&tmp_path);
        let _ = fs::remove_file(&exe_file);
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn atomic_swap_command_structure() {
        let tmp = Path::new("C:\\Users\\test\\AppData\\Local\\LockNote\\abc123.tmp");
        let exe = Path::new("C:\\Program Files\\LockNote\\LockNote.exe");

        let cmd = atomic_swap_command(tmp, exe);
        let program = cmd.get_program().to_str().unwrap();
        assert_eq!(program, "cmd.exe");

        // raw_arg is used, so get_args() returns nothing (raw args aren't in argv)
        // Instead, verify the command builds without panic.
        // The actual command line contains: /c ping ... & move /y "tmp" "exe"
    }

    // ── Additional tests ──

    #[test]
    fn marker_xor_obfuscation() {
        let marker = get_marker();
        // The stored (XORed) form must differ from the raw marker
        assert_ne!(MARKER_XORED, marker);
        // Double-check: every byte should differ (since XOR_KEY != 0)
        for i in 0..MARKER_LEN {
            assert_ne!(MARKER_XORED[i], marker[i]);
        }
    }

    #[test]
    fn find_marker_payload_exactly_min_size() {
        let dir = std::env::temp_dir().join("locknote_test_exact_min");
        let _ = fs::create_dir_all(&dir);
        let file = dir.join("exact_min.bin");

        let marker = get_marker();
        let mut data = vec![0u8; 64]; // exe stub
        data.extend_from_slice(&marker);
        data.extend_from_slice(&[0xBB; MIN_PAYLOAD_SIZE]); // exactly 80 bytes
        fs::write(&file, &data).unwrap();

        let result = read_payload_from_file(&file);
        assert!(result.is_some());
        assert_eq!(result.unwrap().len(), MIN_PAYLOAD_SIZE);

        let _ = fs::remove_file(&file);
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn find_marker_payload_one_byte_under_min() {
        let dir = std::env::temp_dir().join("locknote_test_under_min");
        let _ = fs::create_dir_all(&dir);
        let file = dir.join("under_min.bin");

        let marker = get_marker();
        let mut data = vec![0u8; 64]; // exe stub
        data.extend_from_slice(&marker);
        data.extend_from_slice(&[0xBB; MIN_PAYLOAD_SIZE - 1]); // 79 bytes
        fs::write(&file, &data).unwrap();

        let result = read_payload_from_file(&file);
        assert!(result.is_none());

        let _ = fs::remove_file(&file);
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn find_marker_with_marker_only_no_payload() {
        let dir = std::env::temp_dir().join("locknote_test_marker_only");
        let _ = fs::create_dir_all(&dir);
        let file = dir.join("marker_only.bin");

        let marker = get_marker();
        let mut data = vec![0u8; 64]; // exe stub
        data.extend_from_slice(&marker);
        // No payload bytes after marker
        fs::write(&file, &data).unwrap();

        let result = read_payload_from_file(&file);
        assert!(result.is_none());

        let _ = fs::remove_file(&file);
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn write_data_creates_tmp_file() {
        let dir = std::env::temp_dir().join("locknote_test_write_creates");
        let _ = fs::create_dir_all(&dir);
        let exe_file = dir.join("creates_tmp.exe");

        let exe_stub = vec![0x4Du8, 0x5A, 0x90, 0x00];
        fs::write(&exe_file, &exe_stub).unwrap();

        let payload = vec![0xCC; 100];
        write_data(&exe_file, &payload).unwrap();

        let tmp_path = get_tmp_path(&exe_file);
        assert!(tmp_path.exists(), "tmp file should be created by write_data");

        // Cleanup
        let _ = fs::remove_file(&tmp_path);
        let _ = fs::remove_file(&exe_file);
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn write_then_write_overwrites() {
        let dir = std::env::temp_dir().join("locknote_test_overwrite");
        let _ = fs::create_dir_all(&dir);
        let exe_file = dir.join("overwrite.exe");

        let exe_stub = vec![0x4Du8, 0x5A];
        fs::write(&exe_file, &exe_stub).unwrap();

        // Write payload A
        let payload_a: Vec<u8> = (0..100).collect();
        write_data(&exe_file, &payload_a).unwrap();

        // Write payload B (using the tmp as source now, so point write_data at tmp)
        // Actually write_data reads exe_path, so we need to write B using the same exe.
        // After the first write, the exe still has the original stub (write goes to tmp).
        let payload_b: Vec<u8> = (0..120).map(|i| (i as u8).wrapping_add(0x80)).collect();
        write_data(&exe_file, &payload_b).unwrap();

        // Reading back should give payload B
        let tmp_path = get_tmp_path(&exe_file);
        let recovered = read_payload_from_file(&tmp_path).unwrap();
        assert_eq!(recovered, payload_b);

        // Cleanup
        let _ = fs::remove_file(&tmp_path);
        let _ = fs::remove_file(&exe_file);
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn read_data_prefers_tmp_over_exe() {
        let dir = std::env::temp_dir().join("locknote_test_prefer_tmp");
        let _ = fs::create_dir_all(&dir);
        let exe_file = dir.join("prefer_tmp.exe");

        let marker = get_marker();

        // Create exe with marker + payload A (>= MIN_PAYLOAD_SIZE)
        let payload_a: Vec<u8> = vec![0xAA; 100];
        let mut exe_data = vec![0x4Du8, 0x5A];
        exe_data.extend_from_slice(&marker);
        exe_data.extend_from_slice(&payload_a);
        fs::write(&exe_file, &exe_data).unwrap();

        // Write payload B via write_data (creates .tmp)
        let payload_b: Vec<u8> = vec![0xBB; 100];
        write_data(&exe_file, &payload_b).unwrap();

        // read_data should return payload B from .tmp, not payload A from .exe
        let result = read_data(&exe_file).unwrap();
        assert_eq!(result, payload_b);

        // Cleanup
        let tmp_path = get_tmp_path(&exe_file);
        let _ = fs::remove_file(&tmp_path);
        let _ = fs::remove_file(&exe_file);
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn large_payload_roundtrip() {
        let dir = std::env::temp_dir().join("locknote_test_large");
        let _ = fs::create_dir_all(&dir);
        let exe_file = dir.join("large.exe");

        let exe_stub = vec![0x4Du8, 0x5A, 0x90, 0x00];
        fs::write(&exe_file, &exe_stub).unwrap();

        // 1 MB payload
        let payload: Vec<u8> = (0..1_048_576).map(|i| (i % 251) as u8).collect();
        write_data(&exe_file, &payload).unwrap();

        let recovered = read_data(&exe_file).unwrap();
        assert_eq!(recovered.len(), payload.len());
        assert_eq!(recovered, payload);

        // Cleanup
        let tmp_path = get_tmp_path(&exe_file);
        let _ = fs::remove_file(&tmp_path);
        let _ = fs::remove_file(&exe_file);
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn marker_not_in_xored_form_in_binary() {
        // A file containing MARKER_XORED (the obfuscated bytes) should NOT be
        // found by find_marker, which searches for the real (de-XORed) marker.
        let mut data = vec![0u8; 64];
        data.extend_from_slice(&MARKER_XORED);
        data.extend_from_slice(&[0xCC; 100]);

        let result = find_marker(&data);
        assert!(result.is_none(), "find_marker should not match the XORed form");
    }

    #[test]
    fn tmp_path_in_localappdata() {
        let p = get_tmp_path(Path::new("C:\\test\\app.exe"));
        let local_app_data = std::env::var("LOCALAPPDATA")
            .expect("LOCALAPPDATA must be set");
        let expected_prefix = PathBuf::from(&local_app_data).join("LockNote");
        assert!(
            p.starts_with(&expected_prefix),
            "tmp path {:?} should start with {:?}",
            p, expected_prefix
        );
    }
}
