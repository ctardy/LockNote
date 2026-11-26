// Compression module — GZip compression with Base64 encoding
//
// Format: "LNGZ:" + Base64(GZip(plaintext_utf8))

use base64::Engine;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::{Read, Write};
use zeroize::Zeroize;

const PREFIX: &str = "LNGZ:";

/// Cap on the decompressed output. GZip can be crafted so 1 MB expands to
/// tens of GB ("zip bomb"). HMAC-authenticated data can't be tampered by
/// an external attacker, but a malicious author sharing a vault/note can
/// plant a bomb, so we bound the expansion defensively.
const MAX_DECOMPRESSED_SIZE: u64 = 256 * 1024 * 1024; // 256 MiB

/// Granularity of the length-leak padding added after the base64 payload.
/// The ciphertext length ends up quantised to this multiple, so the file
/// size no longer tracks the compressibility of the plaintext byte-for-byte
/// (CRIME/BREACH-class side channel). Must be a power of two ≤ 64 KiB.
const PAD_BLOCK: usize = 4096;

/// The padding byte. Cannot appear in standard base64 output (alphabet is
/// A-Z, a-z, 0-9, +, /, =). Decompress strips any trailing run of this
/// byte before calling the base64 decoder.
const PAD_BYTE: u8 = b'.';

/// Compress input: UTF-8 -> GZip -> Base64 -> "LNGZ:" prefix
pub fn compress(input: &str) -> String {
    if input.is_empty() {
        return String::new();
    }

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(input.as_bytes()).expect("GZip compression failed");
    let mut compressed = encoder.finish().expect("GZip finish failed");

    let encoded = base64::engine::general_purpose::STANDARD.encode(&compressed);

    // Zeroize the intermediate compressed buffer
    compressed.zeroize();

    // "LNGZ:" + base64(gzip(input)) + padding
    //
    // Without padding, the ciphertext length is a near-direct function of
    // the plaintext's compressibility — an attacker observing multiple
    // versions of the file (backup, sync, git) could recover secrets one
    // character at a time by watching the length shift (CRIME/BREACH).
    //
    // Padding rounds the payload up to the next multiple of PAD_BLOCK,
    // quantising the length signal to 4 KiB steps. Reserve one byte for
    // a separator ('.') so the base64 and the pad can never accidentally
    // concatenate into something the decoder tolerates.
    let header_len = PREFIX.len() + encoded.len();
    let target = header_len.saturating_add(PAD_BLOCK - 1) / PAD_BLOCK * PAD_BLOCK;
    let pad = target.saturating_sub(header_len);

    let mut out = String::with_capacity(header_len + pad);
    out.push_str(PREFIX);
    out.push_str(&encoded);
    for _ in 0..pad {
        out.push(PAD_BYTE as char);
    }
    out
}

/// Decompress input: if "LNGZ:" prefix, strip -> Base64 decode -> GZip inflate -> UTF-8.
/// Otherwise, pass through unchanged (legacy uncompressed data).
pub fn decompress(input: &str) -> String {
    if input.is_empty() {
        return String::new();
    }

    if !input.starts_with(PREFIX) {
        // Legacy pass-through
        return input.to_string();
    }

    // Strip any trailing PAD_BYTE (length-leak padding added by compress).
    // Older data without padding is still accepted because trim_end_matches
    // is a no-op when there's nothing to strip.
    let b64_data = input[PREFIX.len()..].trim_end_matches(PAD_BYTE as char);
    let mut compressed = match base64::engine::general_purpose::STANDARD.decode(b64_data) {
        Ok(data) => data,
        Err(_) => return input.to_string(), // Invalid base64, pass through
    };

    // Wrap the decoder in a Take so we stop at MAX_DECOMPRESSED_SIZE no
    // matter how long the compressed stream promises to expand. +1 lets us
    // detect "exactly reached the cap" and treat it as overflow.
    let decoder = GzDecoder::new(&compressed[..]);
    let mut bounded = decoder.take(MAX_DECOMPRESSED_SIZE + 1);
    let mut decompressed = String::new();
    let result = bounded.read_to_string(&mut decompressed);

    // Zeroize intermediate buffer
    compressed.zeroize();

    match result {
        Ok(_) if (decompressed.len() as u64) > MAX_DECOMPRESSED_SIZE => {
            // Hit the cap — decoder would have produced more. Refuse
            // rather than return a silently-truncated note. Passing the
            // input through would surface raw compressed garbage to the
            // UI, so we hand back an empty string and let the caller
            // treat it as "no content".
            decompressed.zeroize();
            String::new()
        }
        Ok(_) => decompressed,
        Err(_) => input.to_string(), // Decompression failed, pass through
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Known answer (computed with flate2 on miniz_oxide, its default
    /// backend, before the dependency update of 2026-11-26): the bytes
    /// written before every save and backup must not change with a crate
    /// update (zip's "deflate" feature switched flate2 to zlib-rs).
    #[test]
    fn compressed_output_is_pinned() {
        let mut input = String::new();
        for i in 0..40 {
            input.push_str(&format!("line {} \u{e9}t\u{e9} LockNote vault entry {} password=Xy{}z\n", i, i * 7 % 13, i * i % 97));
        }
        let out = compress(&input);
        assert_eq!(out.len(), PAD_BLOCK, "padded to one block");
        let payload = out.trim_end_matches('.');
        assert_eq!(payload, "LNGZ:H4sIAAAAAAAA/42VTU7DQAyF95wiR4jt+V1wA8SabQVZIKIGtQFUbtRz9GIUaWzNbKy3jj7Zfv48Wd+PyzRPt+t+u05P2+vH87Yv0/fha92n5bifLvePn4fz+Wc7vT2+XObfh/WfII/IPUGNYI+gngiNEI8oPVEbETyCh65SQ6KH1B7h2JDkIdIjolWyO/yQcNBhiseEHkmaWHXLDCEX3Qu5y4/DOIq426chaNbeyBUgDRFkZQQWM7MyAVbTkAi7KRZbgu1MVifDflaVjQosqNh+Kmxo1oPmGVeUdCAm2NGozTHjkla9HxZY0qDCccAtrfZKRdxSmyjBllZrLsOWRltRgTUtilTcUjVOZthS1kMVgi0t2pswbGm0X4LglrJaKgG2tOjjKxG3NKk8kvCn1NLOuKVkhQpsadETkgpbmu7MH7r6pd4YCAAA");
        assert_eq!(decompress(&out), input);
    }

    /// The zip crate must not bring flate2's zlib-rs backend (its "deflate"
    /// feature does since zip 9): only "deflate-flate2", so the gzip above
    /// keeps flate2's default backend (miniz_oxide).
    #[test]
    fn zip_keeps_the_default_gzip_backend() {
        let toml = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).unwrap();
        let zip = toml.lines().find(|l| l.starts_with("zip = ")).expect("zip dependency");
        assert!(zip.contains(r#"features = ["deflate-flate2"]"#) && zip.contains("default-features = false"), "{}", zip);
        let flate2 = toml.lines().find(|l| l.starts_with("flate2 = ")).expect("flate2 dependency");
        assert_eq!(flate2.trim(), r#"flate2 = "1""#, "flate2 with its default (rust) backend");
    }

    #[test]
    fn test_roundtrip() {
        let original = "Hello, world! This is a test of compression.";
        let compressed = compress(original);
        assert!(compressed.starts_with(PREFIX));
        let decompressed = decompress(&compressed);
        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_prefix_detection() {
        let compressed = compress("test");
        assert!(compressed.starts_with("LNGZ:"));
    }

    #[test]
    fn test_output_is_padded_to_pad_block() {
        // Short and long inputs should both produce outputs whose total
        // length is a multiple of PAD_BLOCK — this is what quantises the
        // file-size leak against CRIME/BREACH-style attacks.
        for &input in &["a", "short", &"x".repeat(1000), &"y".repeat(9000)] {
            let out = compress(input);
            assert_eq!(
                out.len() % PAD_BLOCK,
                0,
                "compress output length {} is not a multiple of {}",
                out.len(),
                PAD_BLOCK
            );
        }
    }

    #[test]
    fn test_padded_output_roundtrips() {
        let original = "Roundtrip after padding should match byte-for-byte.";
        let out = compress(original);
        assert!(out.ends_with(PAD_BYTE as char), "padding byte must trail the base64 section");
        let back = decompress(&out);
        assert_eq!(back, original);
    }

    #[test]
    fn test_decompress_accepts_legacy_unpadded() {
        // Data produced before the padding scheme (no trailing '.') must
        // still decode cleanly — backward compatibility guarantee.
        let original = "Legacy LNGZ without padding.";
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(original.as_bytes()).unwrap();
        let compressed = encoder.finish().unwrap();
        let encoded = base64::engine::general_purpose::STANDARD.encode(&compressed);
        let legacy = format!("{}{}", PREFIX, encoded); // no trailing pad
        let back = decompress(&legacy);
        assert_eq!(back, original);
    }

    #[test]
    fn test_legacy_passthrough() {
        let legacy = "This is uncompressed legacy data";
        let result = decompress(legacy);
        assert_eq!(result, legacy);
    }

    #[test]
    fn test_empty_input_compress() {
        let result = compress("");
        assert_eq!(result, "");
    }

    #[test]
    fn test_empty_input_decompress() {
        let result = decompress("");
        assert_eq!(result, "");
    }

    #[test]
    fn test_large_text() {
        let large = "The quick brown fox jumps over the lazy dog.\n".repeat(10_000);
        let compressed = compress(&large);
        // Compressed should be smaller than original
        assert!(compressed.len() < large.len());
        let decompressed = decompress(&compressed);
        assert_eq!(decompressed, large);
    }

    #[test]
    fn test_unicode() {
        let unicode_text = "Bonjour le monde! \u{00e9}\u{00e0}\u{00fc}\u{00f6}\n\u{65e5}\u{672c}\u{8a9e}\u{30c6}\u{30b9}\u{30c8}\n\u{1f512}\u{1f511}\u{1f4dd}";
        let compressed = compress(unicode_text);
        let decompressed = decompress(&compressed);
        assert_eq!(decompressed, unicode_text);
    }

    #[test]
    fn test_invalid_base64() {
        let invalid = "LNGZ:!!!not-valid-base64!!!";
        let result = decompress(invalid);
        // Should pass through on error
        assert_eq!(result, invalid);
    }

    #[test]
    fn test_invalid_gzip() {
        // Valid base64 but not valid gzip
        let invalid = format!("LNGZ:{}", base64::engine::general_purpose::STANDARD.encode(b"not gzip data"));
        let result = decompress(&invalid);
        // Should pass through on error
        assert_eq!(result, invalid);
    }

    #[test]
    fn test_single_character() {
        let result = compress("x");
        let back = decompress(&result);
        assert_eq!(back, "x");
    }

    #[test]
    fn test_newlines_and_special_chars() {
        let text = "Line 1\r\nLine 2\nLine 3\t\tTabbed\0NullByte";
        let compressed = compress(text);
        let decompressed = decompress(&compressed);
        assert_eq!(decompressed, text);
    }

    #[test]
    fn test_repeated_compress_decompress() {
        let text = "Repeated test";
        for _ in 0..10 {
            let c = compress(text);
            let d = decompress(&c);
            assert_eq!(d, text);
        }
    }

    /// Roundtrip: compress -> encrypt -> decrypt -> decompress
    #[test]
    fn test_crypto_roundtrip_with_compression() {
        let password = "test-password-123";
        let plaintext = "Hello, compressed LockNote!\nLine 2\nUnicode: \u{00e9}\u{00e0}\u{00fc}";

        // Compress before encryption
        let compressed = compress(plaintext);
        let encrypted = crate::crypto::encrypt(&compressed, password, crate::crypto::DEFAULT_M_COST, crate::crypto::DEFAULT_T_COST, crate::crypto::DEFAULT_P_LANES).unwrap();

        // Decrypt then decompress
        let decrypted = crate::crypto::decrypt(&encrypted, password)
            .expect("decryption should succeed");
        let result = decompress(&decrypted);

        assert_eq!(result, plaintext);
    }

    /// Verify that legacy (uncompressed) encrypted data still decrypts correctly
    #[test]
    fn test_crypto_roundtrip_legacy_no_compression() {
        let password = "legacy-pass";
        let plaintext = "Old data without compression prefix";

        // Encrypt without compression (legacy path)
        let encrypted = crate::crypto::encrypt(plaintext, password, crate::crypto::DEFAULT_M_COST, crate::crypto::DEFAULT_T_COST, crate::crypto::DEFAULT_P_LANES).unwrap();

        // Decrypt then decompress — decompress should pass through
        let decrypted = crate::crypto::decrypt(&encrypted, password)
            .expect("decryption should succeed");
        let result = decompress(&decrypted);

        assert_eq!(result, plaintext);
    }
}
