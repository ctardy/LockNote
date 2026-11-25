// Steganography module — hide/extract encrypted LockNote payloads in image files
//
// Supports JPEG, PNG, and BMP carrier images. The encrypted payload is appended
// after the image's logical end marker. No magic bytes or identifiable markers
// are written — detection relies solely on AES-256-CBC + HMAC-SHA256 verification
// (the existing crypto module), providing plausible deniability.
//
// Layout:
//   [Original image (unchanged)] [Encrypted payload (same wire format as crypto)]

use std::path::Path;

/// Supported carrier image formats.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImageFormat {
    Jpeg,
    Png,
    Bmp,
}

/// Errors returned by stego operations.
#[derive(Debug)]
pub enum StegoError {
    /// The file is not a supported image format.
    UnsupportedFormat,
    /// Could not find the image's logical end marker.
    EndMarkerNotFound,
    /// The carrier image is too small or corrupted.
    ImageTooSmall,
    /// No extra data found after the image boundary.
    NoHiddenData,
    /// Decryption failed (wrong password or not LockNote data).
    DecryptionFailed,
    /// The payload/image size ratio exceeds the warning threshold.
    PayloadTooLarge { ratio_percent: u32 },
    /// I/O error.
    Io(std::io::Error),
}

impl std::fmt::Display for StegoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StegoError::UnsupportedFormat => write!(f, "Unsupported image format"),
            StegoError::EndMarkerNotFound => write!(f, "Could not find end of image data"),
            StegoError::ImageTooSmall => write!(f, "Image file is too small or corrupted"),
            StegoError::NoHiddenData => write!(f, "No hidden data found in this image"),
            StegoError::DecryptionFailed => write!(f, "Wrong password or no LockNote data"),
            StegoError::PayloadTooLarge { ratio_percent } => {
                write!(f, "Payload is {}% of image size (suspicious)", ratio_percent)
            }
            StegoError::Io(e) => write!(f, "I/O error: {}", e),
        }
    }
}

impl From<std::io::Error> for StegoError {
    fn from(e: std::io::Error) -> Self {
        StegoError::Io(e)
    }
}

// ---------------------------------------------------------------------------
// Image format detection
// ---------------------------------------------------------------------------

/// Detect image format from file header (magic bytes).
pub fn detect_format(data: &[u8]) -> Option<ImageFormat> {
    if data.len() < 4 {
        return None;
    }
    // JPEG: starts with FF D8 FF
    if data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF {
        return Some(ImageFormat::Jpeg);
    }
    // PNG: starts with 89 50 4E 47 0D 0A 1A 0A
    if data.len() >= 8 && data[..8] == [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] {
        return Some(ImageFormat::Png);
    }
    // BMP: starts with "BM"
    if data[0] == b'B' && data[1] == b'M' && data.len() >= 14 {
        return Some(ImageFormat::Bmp);
    }
    None
}

/// Detect image format from file extension (case-insensitive fallback).
pub fn detect_format_by_extension(path: &Path) -> Option<ImageFormat> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" | "jpe" | "jfif" => Some(ImageFormat::Jpeg),
        "png" => Some(ImageFormat::Png),
        "bmp" | "dib" => Some(ImageFormat::Bmp),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Find the logical end of image data
// ---------------------------------------------------------------------------

/// Find the byte offset just past the logical end of a JPEG image.
///
/// Scans for the EOI marker (FF D9). We must skip over entropy-coded data
/// carefully by tracking markers — FF D9 inside compressed data is escaped
/// as FF 00 by the JPEG spec, so a true FF D9 is unambiguous.
fn find_jpeg_end(data: &[u8]) -> Option<usize> {
    if data.len() < 4 {
        return None;
    }
    // Scan forward for FFD9 (EOI). In JPEG, within entropy-coded segments
    // (SOS data), 0xFF is always followed by 0x00 (stuffed byte) — a real
    // FF D9 can only appear as the true EOI marker.
    let mut i = 2; // skip SOI (FF D8)
    while i + 1 < data.len() {
        if data[i] == 0xFF && data[i + 1] == 0xD9 {
            return Some(i + 2); // past EOI
        }
        // Skip marker segments to speed up scanning
        if data[i] == 0xFF && data[i + 1] != 0x00 && data[i + 1] != 0xFF {
            let marker = data[i + 1];
            // SOS (0xDA) — start of scan: entropy data follows until EOI
            // We can't skip by length here, just continue byte-by-byte
            if marker == 0xDA {
                i += 2;
                continue;
            }
            // Other markers with length field
            if i + 3 < data.len() && marker != 0x00 && marker != 0x01
                && !(0xD0..=0xD9).contains(&marker)
            {
                let seg_len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
                i += 2 + seg_len;
                continue;
            }
        }
        i += 1;
    }
    None
}

/// Find the byte offset just past the logical end of a PNG image.
///
/// Scans for the IEND chunk. PNG chunks are: length(4) + type(4) + data(length) + crc(4).
/// The IEND chunk has length 0, so it's 12 bytes total.
fn find_png_end(data: &[u8]) -> Option<usize> {
    if data.len() < 8 {
        return None;
    }
    // Skip PNG signature (8 bytes)
    let mut pos = 8;
    while pos + 8 <= data.len() {
        let chunk_len = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        let chunk_type = &data[pos + 4..pos + 8];
        // Total chunk size: 4 (length) + 4 (type) + chunk_len (data) + 4 (CRC)
        let total = 4 + 4 + chunk_len + 4;
        if pos + total > data.len() {
            return None; // truncated
        }
        if chunk_type == b"IEND" {
            return Some(pos + total);
        }
        pos += total;
    }
    None
}

/// Find the byte offset just past the logical end of a BMP image.
///
/// The BMP file header contains the total file size at offset 2 (4 bytes LE).
fn find_bmp_end(data: &[u8]) -> Option<usize> {
    if data.len() < 14 {
        return None;
    }
    let file_size = u32::from_le_bytes([data[2], data[3], data[4], data[5]]) as usize;
    if file_size < 14 || file_size > data.len() {
        // BMP size field points beyond the actual file — can't use it
        return None;
    }
    Some(file_size)
}

/// Detect image format and find the logical end offset.
/// Tries magic bytes first, then falls back to file extension if `path` is provided.
pub fn find_image_end(data: &[u8]) -> Result<(ImageFormat, usize), StegoError> {
    find_image_end_with_path(data, None)
}

/// Like `find_image_end` but with optional path for extension-based fallback.
pub fn find_image_end_with_path(data: &[u8], path: Option<&Path>) -> Result<(ImageFormat, usize), StegoError> {
    let format = detect_format(data)
        .or_else(|| path.and_then(detect_format_by_extension))
        .ok_or(StegoError::UnsupportedFormat)?;
    let end = match format {
        ImageFormat::Jpeg => find_jpeg_end(data).ok_or(StegoError::EndMarkerNotFound)?,
        ImageFormat::Png => find_png_end(data).ok_or(StegoError::EndMarkerNotFound)?,
        ImageFormat::Bmp => find_bmp_end(data).ok_or(StegoError::EndMarkerNotFound)?,
    };
    Ok((format, end))
}

// ---------------------------------------------------------------------------
// Embed (hide payload in image)
// ---------------------------------------------------------------------------

/// Maximum payload-to-image ratio before warning (50%).
const WARN_RATIO_PERCENT: u32 = 50;

/// Maximum payload size (100 MiB).
const MAX_PAYLOAD_SIZE: usize = 100 * 1024 * 1024;

/// Embed an encrypted payload after the image's logical end.
///
/// `image_data` must be a valid JPEG, PNG, or BMP file.
/// `encrypted` is the output of `crypto::encrypt` (wire-format payload).
///
/// Returns the combined image+payload bytes, or an error if the image format
/// is unsupported or the payload is suspiciously large.
pub fn embed(image_data: &[u8], encrypted: &[u8]) -> Result<Vec<u8>, StegoError> {
    embed_with_path(image_data, encrypted, None)
}

/// Like `embed` but with optional path for extension-based format fallback.
pub fn embed_with_path(image_data: &[u8], encrypted: &[u8], path: Option<&Path>) -> Result<Vec<u8>, StegoError> {
    let (_format, image_end) = find_image_end_with_path(image_data, path)?;

    // Check payload size
    if encrypted.len() > MAX_PAYLOAD_SIZE {
        return Err(StegoError::PayloadTooLarge {
            ratio_percent: 100,
        });
    }

    // Warn if payload is disproportionately large compared to the image
    let ratio = (encrypted.len() as u64 * 100) / (image_end as u64).max(1);
    if ratio > WARN_RATIO_PERCENT as u64 {
        return Err(StegoError::PayloadTooLarge {
            ratio_percent: ratio as u32,
        });
    }

    // Build output: original image (up to logical end) + encrypted payload
    let mut output = Vec::with_capacity(image_end + encrypted.len());
    output.extend_from_slice(&image_data[..image_end]);
    output.extend_from_slice(encrypted);
    Ok(output)
}

/// Embed encrypted notes into an image file and write the result.
///
/// This is the high-level function called from the UI:
/// 1. Reads the carrier image
/// 2. Encrypts the note content
/// 3. Appends the encrypted payload after the image end
/// 4. Writes the result to `output_path`
pub fn embed_to_file(
    carrier_path: &Path,
    output_path: &Path,
    plaintext: &str,
    password: &str,
) -> Result<(), StegoError> {
    let image_data = read_carrier_bounded(carrier_path)?;
    let encrypted = crate::crypto::encrypt(
        plaintext,
        password,
        crate::crypto::DEFAULT_M_COST,
        crate::crypto::DEFAULT_T_COST,
        crate::crypto::DEFAULT_P_LANES,
    )
    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
    let result = embed(&image_data, &encrypted)?;
    std::fs::write(output_path, &result)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Extract (recover payload from image)
// ---------------------------------------------------------------------------

/// Extract and decrypt hidden data from an image file.
///
/// Returns `Ok(plaintext)` if data was found and decrypted successfully.
/// Returns `Err(NoHiddenData)` if no extra bytes exist after the image boundary.
/// Returns `Err(DecryptionFailed)` if extra bytes exist but decryption fails
/// (wrong password or not LockNote data — intentionally ambiguous).
pub fn extract(file_data: &[u8], password: &str) -> Result<String, StegoError> {
    let (_format, image_end) = find_image_end(file_data)?;

    // Check if there's data after the image boundary
    if image_end >= file_data.len() {
        return Err(StegoError::NoHiddenData);
    }

    let payload = &file_data[image_end..];

    // Minimum encrypted payload size check
    if payload.len() < crate::crypto::MIN_PAYLOAD_SIZE {
        return Err(StegoError::NoHiddenData);
    }

    // Attempt decryption — HMAC verification confirms validity.
    // Any crypto error is flattened to DecryptionFailed so callers cannot
    // distinguish wrong-password from corrupted-data (anti-oracle).
    match crate::crypto::decrypt(payload, password) {
        Ok(plaintext) => Ok(plaintext),
        Err(_) => Err(StegoError::DecryptionFailed),
    }
}

/// Extract hidden data from an image file on disk.
pub fn extract_from_file(image_path: &Path, password: &str) -> Result<String, StegoError> {
    let file_data = read_carrier_bounded(image_path)?;
    extract(&file_data, password)
}

/// Maximum accepted carrier image size. The parsers do not decode pixels, so
/// the cap can be generous, but bounded — without it a multi-GB file handed
/// to the user (by an attacker on a shared drive, or a malformed selection)
/// would land entirely in RAM and crash the process (OOM).
const MAX_CARRIER_SIZE: u64 = 256 * 1024 * 1024; // 256 MiB

/// Read an image file into memory, refusing inputs above `MAX_CARRIER_SIZE`.
fn read_carrier_bounded(path: &Path) -> Result<Vec<u8>, StegoError> {
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > MAX_CARRIER_SIZE {
        return Err(StegoError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "carrier image exceeds the {}-byte size cap (got {} bytes)",
                MAX_CARRIER_SIZE,
                metadata.len()
            ),
        )));
    }
    std::fs::read(path).map_err(StegoError::Io)
}

/// Check whether a file appears to have data appended after the image boundary,
/// without attempting decryption. Useful for UI hints.
pub fn has_hidden_data(file_data: &[u8]) -> bool {
    if let Ok((_format, image_end)) = find_image_end(file_data) {
        let remaining = file_data.len() - image_end;
        remaining >= crate::crypto::MIN_PAYLOAD_SIZE
    } else {
        false
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- JPEG helpers ---

    /// Build a valid JPEG large enough for embed tests.
    /// SOI + APP0 (with padding to ~1 KB) + EOI
    fn minimal_jpeg() -> Vec<u8> {
        let mut buf = vec![0xFF, 0xD8]; // SOI
        // APP0 marker with 1000 bytes of padding
        buf.push(0xFF);
        buf.push(0xE0);
        let seg_len: u16 = 1000;
        buf.extend_from_slice(&seg_len.to_be_bytes());
        buf.extend(std::iter::repeat(0x00).take(seg_len as usize - 2));
        buf.push(0xFF);
        buf.push(0xD9); // EOI
        buf
    }

    // --- PNG helpers ---

    /// Build a valid PNG large enough for embed tests.
    /// Signature + IHDR + a padding tEXt chunk (~1 KB) + IEND
    fn minimal_png() -> Vec<u8> {
        let mut buf = Vec::new();
        // PNG signature
        buf.extend_from_slice(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
        // IHDR chunk (13 bytes data)
        let ihdr_data = [0u8; 13];
        let ihdr_crc = crc32(&b"IHDR"[..], &ihdr_data);
        buf.extend_from_slice(&13u32.to_be_bytes());
        buf.extend_from_slice(b"IHDR");
        buf.extend_from_slice(&ihdr_data);
        buf.extend_from_slice(&ihdr_crc.to_be_bytes());
        // tEXt chunk with ~1000 bytes of padding
        let text_data = vec![0x41u8; 1000]; // 'A' repeated
        let text_crc = crc32(b"tEXt", &text_data);
        buf.extend_from_slice(&(text_data.len() as u32).to_be_bytes());
        buf.extend_from_slice(b"tEXt");
        buf.extend_from_slice(&text_data);
        buf.extend_from_slice(&text_crc.to_be_bytes());
        // IEND chunk (0 bytes data)
        let iend_crc = crc32(&b"IEND"[..], &[]);
        buf.extend_from_slice(&0u32.to_be_bytes());
        buf.extend_from_slice(b"IEND");
        buf.extend_from_slice(&iend_crc.to_be_bytes());
        buf
    }

    /// Simple CRC-32 for PNG chunk validation (ISO 3309 / ITU-T V.42).
    fn crc32(chunk_type: &[u8], chunk_data: &[u8]) -> u32 {
        let mut crc: u32 = 0xFFFF_FFFF;
        for &b in chunk_type.iter().chain(chunk_data.iter()) {
            crc ^= b as u32;
            for _ in 0..8 {
                if crc & 1 != 0 {
                    crc = (crc >> 1) ^ 0xEDB8_8320;
                } else {
                    crc >>= 1;
                }
            }
        }
        crc ^ 0xFFFF_FFFF
    }

    // --- BMP helpers ---

    /// Build a valid BMP large enough for embed tests (20x20 pixel, 24-bit).
    fn minimal_bmp() -> Vec<u8> {
        let width: i32 = 20;
        let height: i32 = 20;
        let row_size = ((width * 3 + 3) / 4) * 4; // padded to 4-byte boundary = 60
        let pixel_data_size = row_size * height; // 1200
        let file_size: u32 = (14 + 40 + pixel_data_size) as u32;
        let mut buf = Vec::new();
        // File header (14 bytes)
        buf.extend_from_slice(b"BM");
        buf.extend_from_slice(&file_size.to_le_bytes());
        buf.extend_from_slice(&[0u8; 4]); // reserved
        buf.extend_from_slice(&54u32.to_le_bytes()); // pixel data offset
        // DIB header (BITMAPINFOHEADER, 40 bytes)
        buf.extend_from_slice(&40u32.to_le_bytes()); // header size
        buf.extend_from_slice(&width.to_le_bytes());
        buf.extend_from_slice(&height.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes()); // color planes
        buf.extend_from_slice(&24u16.to_le_bytes()); // bpp
        buf.extend_from_slice(&[0u8; 24]); // rest of DIB header
        // Pixel data
        buf.extend(std::iter::repeat(0xAA).take(pixel_data_size as usize));
        assert_eq!(buf.len(), file_size as usize);
        buf
    }

    // Use fast Argon2id params for tests
    const TEST_M: u32 = 256;
    const TEST_T: u32 = 1;
    const TEST_P: u32 = 1;

    fn test_encrypt(plaintext: &str, password: &str) -> Vec<u8> {
        crate::crypto::encrypt(plaintext, password, TEST_M, TEST_T, TEST_P).unwrap()
    }

    // --- Format detection tests ---

    #[test]
    fn detect_jpeg() {
        assert_eq!(detect_format(&minimal_jpeg()), Some(ImageFormat::Jpeg));
    }

    #[test]
    fn detect_png() {
        assert_eq!(detect_format(&minimal_png()), Some(ImageFormat::Png));
    }

    #[test]
    fn detect_bmp() {
        assert_eq!(detect_format(&minimal_bmp()), Some(ImageFormat::Bmp));
    }

    #[test]
    fn detect_unknown() {
        assert_eq!(detect_format(&[0x00, 0x01, 0x02, 0x03]), None);
    }

    // --- Image end detection tests ---

    #[test]
    fn jpeg_end() {
        let jpeg = minimal_jpeg();
        let (fmt, end) = find_image_end(&jpeg).unwrap();
        assert_eq!(fmt, ImageFormat::Jpeg);
        assert_eq!(end, jpeg.len());
    }

    #[test]
    fn png_end() {
        let png = minimal_png();
        let (fmt, end) = find_image_end(&png).unwrap();
        assert_eq!(fmt, ImageFormat::Png);
        assert_eq!(end, png.len());
    }

    #[test]
    fn bmp_end() {
        let bmp = minimal_bmp();
        let (fmt, end) = find_image_end(&bmp).unwrap();
        assert_eq!(fmt, ImageFormat::Bmp);
        assert_eq!(end, bmp.len());
    }

    // --- Embed/extract round-trip tests ---

    #[test]
    fn round_trip_jpeg() {
        let jpeg = minimal_jpeg();
        let encrypted = test_encrypt("secret notes", "password123");
        let combined = embed(&jpeg, &encrypted).unwrap();
        assert!(combined.len() > jpeg.len());
        // Image portion is unchanged
        assert_eq!(&combined[..jpeg.len()], &jpeg[..]);
        // Extract and decrypt
        let result = extract(&combined, "password123").unwrap();
        assert_eq!(result, "secret notes");
    }

    #[test]
    fn round_trip_png() {
        let png = minimal_png();
        let encrypted = test_encrypt("hidden in png", "pass");
        let combined = embed(&png, &encrypted).unwrap();
        assert_eq!(&combined[..png.len()], &png[..]);
        let result = extract(&combined, "pass").unwrap();
        assert_eq!(result, "hidden in png");
    }

    #[test]
    fn round_trip_bmp() {
        let bmp = minimal_bmp();
        let encrypted = test_encrypt("bmp payload", "bmppass");
        let combined = embed(&bmp, &encrypted).unwrap();
        assert_eq!(&combined[..bmp.len()], &bmp[..]);
        let result = extract(&combined, "bmppass").unwrap();
        assert_eq!(result, "bmp payload");
    }

    #[test]
    fn round_trip_unicode() {
        let jpeg = minimal_jpeg();
        let text = "Café ☕ 日本語 données chiffrées 🔐";
        let encrypted = test_encrypt(text, "motdepasse");
        let combined = embed(&jpeg, &encrypted).unwrap();
        let result = extract(&combined, "motdepasse").unwrap();
        assert_eq!(result, text);
    }

    // --- Error case tests ---

    #[test]
    fn extract_no_hidden_data() {
        let jpeg = minimal_jpeg();
        match extract(&jpeg, "pass") {
            Err(StegoError::NoHiddenData) => {}
            other => panic!("Expected NoHiddenData, got {:?}", other),
        }
    }

    #[test]
    fn extract_wrong_password() {
        let jpeg = minimal_jpeg();
        let encrypted = test_encrypt("secret", "right");
        let combined = embed(&jpeg, &encrypted).unwrap();
        match extract(&combined, "wrong") {
            Err(StegoError::DecryptionFailed) => {}
            other => panic!("Expected DecryptionFailed, got {:?}", other),
        }
    }

    #[test]
    fn unsupported_format() {
        let data = vec![0x00; 100];
        match find_image_end(&data) {
            Err(StegoError::UnsupportedFormat) => {}
            other => panic!("Expected UnsupportedFormat, got {:?}", other),
        }
    }

    #[test]
    fn has_hidden_data_true() {
        let jpeg = minimal_jpeg();
        let encrypted = test_encrypt("data", "pass");
        let combined = embed(&jpeg, &encrypted).unwrap();
        assert!(has_hidden_data(&combined));
    }

    #[test]
    fn has_hidden_data_false() {
        let jpeg = minimal_jpeg();
        assert!(!has_hidden_data(&jpeg));
    }

    #[test]
    fn has_hidden_data_too_small() {
        let mut jpeg = minimal_jpeg();
        // Add a few bytes (less than MIN_PAYLOAD_SIZE)
        jpeg.extend_from_slice(&[0x00; 10]);
        assert!(!has_hidden_data(&jpeg));
    }

    // --- Extension-based detection tests ---

    #[test]
    fn detect_by_extension_jpg() {
        assert_eq!(detect_format_by_extension(Path::new("photo.jpg")), Some(ImageFormat::Jpeg));
    }

    #[test]
    fn detect_by_extension_jpeg() {
        assert_eq!(detect_format_by_extension(Path::new("photo.jpeg")), Some(ImageFormat::Jpeg));
    }

    #[test]
    fn detect_by_extension_jfif() {
        assert_eq!(detect_format_by_extension(Path::new("photo.jfif")), Some(ImageFormat::Jpeg));
    }

    #[test]
    fn detect_by_extension_png() {
        assert_eq!(detect_format_by_extension(Path::new("image.png")), Some(ImageFormat::Png));
    }

    #[test]
    fn detect_by_extension_bmp() {
        assert_eq!(detect_format_by_extension(Path::new("image.bmp")), Some(ImageFormat::Bmp));
    }

    #[test]
    fn detect_by_extension_dib() {
        assert_eq!(detect_format_by_extension(Path::new("image.dib")), Some(ImageFormat::Bmp));
    }

    #[test]
    fn detect_by_extension_case_insensitive() {
        assert_eq!(detect_format_by_extension(Path::new("PHOTO.JPG")), Some(ImageFormat::Jpeg));
        assert_eq!(detect_format_by_extension(Path::new("Image.PNG")), Some(ImageFormat::Png));
    }

    #[test]
    fn detect_by_extension_unknown() {
        assert_eq!(detect_format_by_extension(Path::new("file.txt")), None);
        assert_eq!(detect_format_by_extension(Path::new("file.gif")), None);
    }

    #[test]
    fn detect_by_extension_no_extension() {
        assert_eq!(detect_format_by_extension(Path::new("noext")), None);
    }

    // --- Format detection edge cases ---

    #[test]
    fn detect_format_too_short() {
        assert_eq!(detect_format(&[0xFF, 0xD8]), None);
        assert_eq!(detect_format(&[]), None);
    }

    // --- find_image_end_with_path fallback ---

    #[test]
    fn find_end_with_path_fallback() {
        // BMP data but strip magic bytes, use path for detection
        let bmp = minimal_bmp();
        // Normal detection works
        let (fmt, end) = find_image_end_with_path(&bmp, Some(Path::new("test.bmp"))).unwrap();
        assert_eq!(fmt, ImageFormat::Bmp);
        assert_eq!(end, bmp.len());
    }

    // --- Embed edge cases ---

    #[test]
    fn embed_with_path_works() {
        let jpeg = minimal_jpeg();
        let encrypted = test_encrypt("via path", "pw");
        let result = embed_with_path(&jpeg, &encrypted, Some(Path::new("test.jpg"))).unwrap();
        assert!(result.len() > jpeg.len());
        let text = extract(&result, "pw").unwrap();
        assert_eq!(text, "via path");
    }

    #[test]
    fn embed_empty_content() {
        let jpeg = minimal_jpeg();
        let encrypted = test_encrypt("", "pw");
        let combined = embed(&jpeg, &encrypted).unwrap();
        let result = extract(&combined, "pw").unwrap();
        assert_eq!(result, "");
    }

    // --- Extract edge cases ---

    #[test]
    fn extract_truncated_data() {
        let data = vec![0x00; 3];
        match extract(&data, "pw") {
            Err(StegoError::UnsupportedFormat) => {}
            other => panic!("Expected UnsupportedFormat, got {:?}", other),
        }
    }

    // --- has_hidden_data edge cases ---

    #[test]
    fn has_hidden_data_not_an_image() {
        assert!(!has_hidden_data(&[0x00, 0x01, 0x02, 0x03]));
    }

    // --- Payload size warning ---

    #[test]
    fn payload_too_large_ratio() {
        let jpeg = minimal_jpeg();
        // Create a payload larger than the image (> 50% ratio)
        let big_payload = vec![0xAA; jpeg.len() * 2];
        match embed(&jpeg, &big_payload) {
            Err(StegoError::PayloadTooLarge { .. }) => {}
            other => panic!("Expected PayloadTooLarge, got {:?}", other),
        }
    }
}
