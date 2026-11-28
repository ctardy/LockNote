// Compression module — GZip compression with Base64 encoding
//
// Format: "LNGZ:" + Base64(GZip(plaintext_utf8))
//
// Plaintext copies (issue #42): the gzip output and the inflated text grow
// in a `WipingBuf` (each growth wipes the old allocation), the inflater
// reads into a wiped heap chunk, and the compressed text is encoded straight
// into a result allocated at its exact size. Out of reach: the state of
// flate2 itself (the deflate window and the inflate dictionary hold up to
// 32 KiB of plaintext, its internal output buffer some compressed bytes),
// freed unwiped by the crate.

use base64::Engine;
use crate::crypto::wiping::{WipingBuf, WipingString};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::{Read, Write};
use zeroize::{Zeroize, Zeroizing};

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

    // The gzip output grows in a WipingBuf, wiped on drop.
    let mut encoder = GzEncoder::new(WipingBuf::with_capacity(input.len() / 2 + 64), Compression::default());
    encoder.write_all(input.as_bytes()).expect("GZip compression failed");
    let compressed = encoder.finish().expect("GZip finish failed");

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
    let encoded_len = base64::encoded_len(compressed.len(), true).expect("GZip output too large for base64");
    let header_len = PREFIX.len() + encoded_len;
    let target = header_len.saturating_add(PAD_BLOCK - 1) / PAD_BLOCK * PAD_BLOCK;
    let pad = target.saturating_sub(header_len);

    // Allocated at its exact final size, the base64 encoded straight into it.
    let mut out = WipingString::with_capacity(header_len + pad);
    out.push_str(PREFIX);
    out.push_base64(compressed.as_bytes());
    for _ in 0..pad {
        out.push(PAD_BYTE as char);
    }
    debug_assert_eq!(out.len(), out.capacity(), "the compressed text outgrew its buffer");
    out.into_string()
}

/// Decompress input: if "LNGZ:" prefix, strip -> Base64 decode -> GZip inflate -> UTF-8.
/// Otherwise, pass through unchanged (legacy uncompressed data).
///
/// Every text returned holds the plaintext: the caller wraps it. A
/// pass-through is a copy of `input` made at its exact size.
pub fn decompress(input: &str) -> String {
    inflate_bounded(input, MAX_DECOMPRESSED_SIZE)
}

/// `decompress` with the cap on the inflated size as a parameter (tests use
/// a small one).
fn inflate_bounded(input: &str, max_size: u64) -> String {
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
    let compressed: Zeroizing<Vec<u8>> = match base64::engine::general_purpose::STANDARD.decode(b64_data) {
        Ok(data) => Zeroizing::new(data),
        Err(_) => return input.to_string(), // Invalid base64, pass through
    };

    // Wrap the decoder in a Take so we stop at max_size no matter how long
    // the compressed stream promises to expand. +1 lets us detect "exactly
    // reached the cap" and treat it as overflow.
    let mut bounded = GzDecoder::new(&compressed[..]).take(max_size + 1);
    // The text grows in a WipingBuf; the decoder writes into a heap chunk
    // wiped on drop (`read_to_string` would grow a String, and `io::copy`
    // goes through an unwiped stack buffer).
    let mut inflated = WipingBuf::with_capacity(compressed.len().saturating_mul(4));
    let mut chunk: Zeroizing<Vec<u8>> = Zeroizing::new(vec![0u8; 32 * 1024]);
    let result = loop {
        match bounded.read(&mut chunk[..]) {
            Ok(0) => break Ok(()),
            Ok(n) => inflated.extend_from_slice(&chunk[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => break Err(e),
        }
    };
    if result.is_err() {
        return input.to_string(); // Decompression failed, pass through
    }

    // Same order as `read_to_string`: a text that is not UTF-8 passes
    // through, then the cap.
    let mut text = match String::from_utf8(inflated.into_vec()) {
        Ok(text) => text,
        Err(e) => {
            e.into_bytes().zeroize();
            return input.to_string();
        }
    };
    if (text.len() as u64) > max_size {
        // Hit the cap — decoder would have produced more. Refuse
        // rather than return a silently-truncated note. Passing the
        // input through would surface raw compressed garbage to the
        // UI, so we hand back an empty string and let the caller
        // treat it as "no content".
        text.zeroize();
        return String::new();
    }
    text
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

    // ---- Issue #42 characterisation (written and green before the change) ----

    fn sha256_hex(b: &[u8]) -> String {
        use sha2::Digest;
        sha2::Sha256::digest(b).iter().map(|x| format!("{:02x}", x)).collect()
    }

    /// Text that compresses badly (a linear congruential sequence over
    /// letters, digits and accented characters): its gzip is far larger than
    /// the encoder's 32 KiB output chunks, so the output buffer grows many times.
    fn issue42_large_input() -> String {
        let alphabet: Vec<char> = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 \n\u{e9}\u{e8}\u{e0}\u{fc}\u{6f22}".chars().collect();
        let mut x: u32 = 12345;
        let mut s = String::new();
        for _ in 0..300_000 {
            x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            s.push(alphabet[(x >> 16) as usize % alphabet.len()]);
        }
        s
    }

    #[test]
    fn issue42_compress_output_of_a_large_input_is_pinned() {
        let input = issue42_large_input();
        let out = compress(&input);
        assert_eq!(out.len(), 327680);
        assert_eq!(sha256_hex(out.as_bytes()), "75fe4c706f8a396cced15dd5741ca965cd8affd3fbeaf7a6d016ec5c452ba993");
        assert_eq!(decompress(&out), input);
    }

    #[test]
    fn issue42_compress_output_of_short_inputs_is_pinned() {
        assert_eq!(compress(""), "");
        let x = compress("x");
        assert_eq!(x.len(), PAD_BLOCK);
        assert_eq!(x.trim_end_matches('.'), "LNGZ:H4sIAAAAAAAA/6sAAIMW3IwBAAAA");
        let crlf = compress("a\r\n\u{0}\u{1f512}");
        assert_eq!(crlf.trim_end_matches('.'), "LNGZ:H4sIAAAAAAAA/0vk5WL4MH/KJAD2aSrCCAAAAA==");
        // The padding added for sizes around a block boundary.
        let mut pads = Vec::new();
        for n in [3000usize, 3060, 3070, 3080, 6000] {
            let s: String = issue42_large_input().chars().take(n).collect();
            let out = compress(&s);
            pads.push((out.len(), out.len() - out.trim_end_matches('.').len()));
        }
        assert_eq!(format!("{:?}", pads), "[(4096, 739), (4096, 675), (4096, 663), (4096, 655), (8192, 1555)]");
    }

    /// What `decompress` hands back for every kind of input: the legacy
    /// pass-through, invalid base64, invalid gzip, a truncated stream, gzip
    /// of bytes that are not UTF-8, a stream followed by garbage, the bare
    /// prefix and the prefix with only padding.
    #[test]
    fn issue42_decompress_edge_cases_are_pinned() {
        let gz = |b: &[u8]| {
            let mut e = GzEncoder::new(Vec::new(), Compression::default());
            e.write_all(b).unwrap();
            e.finish().unwrap()
        };
        let lngz = |b: &[u8]| format!("{}{}", PREFIX, base64::engine::general_purpose::STANDARD.encode(b));

        // Legacy text, with what looks like a prefix further on.
        for legacy in ["plain note", "lngz:lower", " LNGZ:space first", "\u{e9}\r\nLNGZ:", "LNG"] {
            assert_eq!(decompress(legacy), legacy);
        }
        let not_utf8 = lngz(&gz(&[0x66, 0xff, 0xfe, 0x67]));
        assert_eq!(decompress(&not_utf8), not_utf8, "gzip of non-UTF-8 bytes passes through");
        let full = gz("truncated stream text".as_bytes());
        let truncated = lngz(&full[..full.len() - 6]);
        assert_eq!(decompress(&truncated), truncated, "a truncated stream passes through");
        let mut trailing = gz(b"with trailing bytes");
        trailing.extend_from_slice(b"garbage");
        assert_eq!(decompress(&lngz(&trailing)), "with trailing bytes");
        assert_eq!(decompress("LNGZ:"), "LNGZ:");
        assert_eq!(decompress("LNGZ:...."), "LNGZ:....");
        let padded_in_middle = format!("{}..{}", PREFIX, base64::engine::general_purpose::STANDARD.encode(gz(b"m")));
        assert_eq!(decompress(&padded_in_middle), padded_in_middle);
        // Two gzip members back to back: only the first is read.
        let mut two = gz(b"one");
        two.extend_from_slice(&gz(b"two"));
        assert_eq!(decompress(&lngz(&two)), "one");
        assert_eq!(decompress(&lngz(&gz(b""))), "");
    }

    // ---- Issue #42 ----

    #[test]
    fn issue42_compressed_text_is_allocated_at_its_exact_size() {
        let large = issue42_large_input();
        for input in ["x", "a\r\n\u{0}\u{1f512}", large.as_str(), &large[..3070], &large[..6000]] {
            let out = compress(input);
            assert_eq!(out.capacity(), out.len(), "{}", input.len());
        }
    }

    /// The cap on the inflated size, with a small one: at the cap the text
    /// is kept, over it nothing is, and a text that is not UTF-8 passes
    /// through first (the order of `read_to_string`).
    #[test]
    fn issue42_inflate_cap_and_order() {
        let gz = |b: &[u8]| {
            let mut e = GzEncoder::new(Vec::new(), Compression::default());
            e.write_all(b).unwrap();
            e.finish().unwrap()
        };
        let lngz = |b: &[u8]| format!("{}{}", PREFIX, base64::engine::general_purpose::STANDARD.encode(gz(b)));
        assert_eq!(inflate_bounded(&lngz(b"abcdef"), 6), "abcdef");
        assert_eq!(inflate_bounded(&lngz(b"abcdef"), 5), "");
        let not_utf8 = lngz(&[0xffu8; 10]);
        assert_eq!(inflate_bounded(&not_utf8, 5), not_utf8);
        // Larger than a read chunk and than the first capacity guess.
        let big = "z\u{e9}".repeat(100_000);
        assert_eq!(inflate_bounded(&compress(&big), MAX_DECOMPRESSED_SIZE), big);
    }

    fn issue42_fn_body(src: &str, start: &str) -> String {
        let at = src.find(start).unwrap_or_else(|| panic!("{start}"));
        let len = src[at..].find("\n}\n").unwrap();
        src[at..at + len].to_string()
    }

    /// The wiping tests cannot watch (freed memory), pinned in the source.
    #[test]
    fn issue42_compression_buffers_wipe_themselves() {
        let src = crate::read_source("src/features/compression/mod.rs");
        let compress = issue42_fn_body(&src, "pub fn compress(");
        assert!(compress.contains("let mut encoder = GzEncoder::new(WipingBuf::with_capacity(input.len() / 2 + 64), Compression::default());"));
        assert!(compress.contains("let mut out = WipingString::with_capacity(header_len + pad);"));
        assert!(compress.contains("out.push_base64(compressed.as_bytes());"));
        assert!(!compress.contains(".encode("), "a base64 String in between");
        let inflate = issue42_fn_body(&src, "fn inflate_bounded(");
        assert!(inflate.contains("let compressed: Zeroizing<Vec<u8>> = match base64::engine::general_purpose::STANDARD.decode(b64_data) {"));
        assert!(inflate.contains("let mut inflated = WipingBuf::with_capacity(compressed.len().saturating_mul(4));"));
        assert!(inflate.contains("let mut chunk: Zeroizing<Vec<u8>> = Zeroizing::new(vec![0u8; 32 * 1024]);"));
        assert!(inflate.contains("Ok(n) => inflated.extend_from_slice(&chunk[..n]),"));
        assert!(inflate.contains("let mut text = match String::from_utf8(inflated.into_vec()) {"));
        assert!(inflate.contains("e.into_bytes().zeroize();"));
        assert!(inflate.contains("text.zeroize();\n        return String::new();"));
        let code: String = inflate.lines().filter(|l| !l.trim_start().starts_with("//")).collect();
        assert!(!code.contains("read_to_string") && !code.contains("io::copy"), "a growing String or a stack buffer");
        assert!(src.contains("pub fn decompress(input: &str) -> String {\n    inflate_bounded(input, MAX_DECOMPRESSED_SIZE)\n}"));
    }
}
