// TOTP (Time-based One-Time Password) module — RFC 6238 / RFC 4226
//
// Provides TOTP generation, verification, Base32 encoding/decoding,
// OtpAuth URI handling, and entry serialization.

use hmac::{Hmac, KeyInit, Mac};
use sha1::Sha1;
use zeroize::{Zeroize, ZeroizeOnDrop};

type HmacSha1 = Hmac<Sha1>;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const TOTP_PERIOD: u64 = 30;
pub const TOTP_DIGITS: u32 = 6;
pub const TOTP_SECRET_SIZE: usize = 20;
/// Shortest secret accepted when the user pastes one: 80 bits, the size many
/// sites give (RFC 4226 asks for 128; refusing below 80 catches a paste cut
/// short without refusing real secrets).
pub const MIN_SECRET_BYTES: usize = 10;

// ---------------------------------------------------------------------------
// TotpEntry model
// ---------------------------------------------------------------------------

/// A TOTP entry.
///
/// The `secret` field (Base32-encoded seed) is automatically
/// zeroized when the entry is dropped (memory-dump protection).
/// `name` is considered non-sensitive (displayed in the UI) and
/// is skipped by the zeroizer.
// Debug is implemented manually below so `{:?}` redacts the base32
// seed while still allowing assert_eq! in tests. Never derive it.
#[derive(Clone, PartialEq, Zeroize, ZeroizeOnDrop)]
pub struct TotpEntry {
    #[zeroize(skip)]
    pub name: String,
    pub secret: String, // Base32-encoded
}

impl std::fmt::Debug for TotpEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TotpEntry")
            .field("name", &self.name)
            .field("secret", &"<redacted>")
            .finish()
    }
}

// ---------------------------------------------------------------------------
// Base32 (RFC 4648)
// ---------------------------------------------------------------------------

const BASE32_ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// Encode raw bytes to Base32 (uppercase, no padding).
pub fn base32_encode(data: &[u8]) -> String {
    if data.is_empty() {
        return String::new();
    }
    let mut result = String::with_capacity((data.len() * 8 + 4) / 5);
    let mut buffer: u64 = 0;
    let mut bits: u32 = 0;

    for &byte in data {
        buffer = (buffer << 8) | (byte as u64);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            let idx = ((buffer >> bits) & 0x1F) as usize;
            result.push(BASE32_ALPHABET[idx] as char);
        }
    }
    // Remaining bits (left-shifted to fill a 5-bit group)
    if bits > 0 {
        let idx = ((buffer << (5 - bits)) & 0x1F) as usize;
        result.push(BASE32_ALPHABET[idx] as char);
    }
    result
}

/// Decode a Base32 string. Accepts lowercase and padding (`=`).
/// Returns `None` on invalid characters.
pub fn base32_decode(encoded: &str) -> Option<Vec<u8>> {
    if encoded.is_empty() {
        return Some(Vec::new());
    }

    let clean: String = encoded
        .chars()
        .filter(|&c| c != '=')
        .collect();

    if clean.is_empty() {
        return Some(Vec::new());
    }

    let mut buffer: u64 = 0;
    let mut bits: u32 = 0;
    let mut result = Vec::with_capacity(clean.len() * 5 / 8);

    for ch in clean.chars() {
        let val = match ch.to_ascii_uppercase() {
            c @ 'A'..='Z' => (c as u8) - b'A',
            c @ '2'..='7' => (c as u8) - b'2' + 26,
            _ => return None,
        };
        buffer = (buffer << 5) | (val as u64);
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            result.push(((buffer >> bits) & 0xFF) as u8);
        }
    }
    Some(result)
}

// ---------------------------------------------------------------------------
// TOTP core (RFC 6238)
// ---------------------------------------------------------------------------

/// Generate a 6-digit TOTP code for the given Base32-encoded secret and Unix timestamp.
pub fn generate_code(secret_base32: &str, timestamp: u64) -> Option<String> {
    let mut secret_bytes = base32_decode(secret_base32)?;
    if secret_bytes.is_empty() {
        secret_bytes.zeroize();
        return None;
    }

    let counter = timestamp / TOTP_PERIOD;
    let counter_bytes = counter.to_be_bytes();

    let mut mac = HmacSha1::new_from_slice(&secret_bytes).ok()?;
    secret_bytes.zeroize();

    mac.update(&counter_bytes);
    let hash = mac.finalize().into_bytes();

    let offset = (hash[19] & 0x0F) as usize;
    let code = (((hash[offset] & 0x7F) as u32) << 24
        | (hash[offset + 1] as u32) << 16
        | (hash[offset + 2] as u32) << 8
        | (hash[offset + 3] as u32))
        % 1_000_000;

    Some(format!("{:06}", code))
}

/// Generate a TOTP code for the current time.
pub fn generate_code_now(secret_base32: &str) -> Option<String> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before Unix epoch")
        .as_secs();
    generate_code(secret_base32, timestamp)
}

/// Verify a TOTP code with ±1 period tolerance.
pub fn verify(secret_base32: &str, code: &str, timestamp: u64) -> bool {
    // Check current period and ±1
    for offset in &[0i64, -1, 1] {
        let ts = if *offset < 0 {
            timestamp.saturating_sub(TOTP_PERIOD)
        } else if *offset > 0 {
            timestamp.saturating_add(TOTP_PERIOD)
        } else {
            timestamp
        };
        if let Some(ref expected) = generate_code(secret_base32, ts) {
            if constant_time_eq(expected.as_bytes(), code.as_bytes()) {
                return true;
            }
        }
    }
    false
}

/// Constant-time comparison for code strings.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Seconds remaining until the next TOTP period.
pub fn remaining_seconds() -> u32 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before Unix epoch")
        .as_secs();
    (TOTP_PERIOD - (now % TOTP_PERIOD)) as u32
}

// ---------------------------------------------------------------------------
// Secret generation
// ---------------------------------------------------------------------------

/// Generate a random 20-byte secret and return it as a Base32-encoded string.
pub fn generate_secret() -> String {
    use rand::RngCore;
    let mut buf = [0u8; TOTP_SECRET_SIZE];
    rand::rng().fill_bytes(&mut buf);
    let encoded = base32_encode(&buf);
    buf.zeroize();
    encoded
}

// ---------------------------------------------------------------------------
// OtpAuth URI
// ---------------------------------------------------------------------------

/// Build an `otpauth://totp/` URI.
pub fn build_uri(label: &str, secret: &str, issuer: Option<&str>) -> String {
    let encoded_label = uri_encode(label);
    let mut uri = format!("otpauth://totp/{}?secret={}", encoded_label, secret);
    if let Some(iss) = issuer {
        uri.push_str(&format!("&issuer={}", uri_encode(iss)));
    }
    uri
}

/// Parse an `otpauth://totp/` URI.
/// Returns `(label, secret, Option<issuer>)`.
pub fn parse_uri(uri: &str) -> Option<(String, String, Option<String>)> {
    let rest = uri.strip_prefix("otpauth://totp/")?;

    let (label_part, query) = rest.split_once('?')?;
    let label = uri_decode(label_part);

    let mut secret: Option<String> = None;
    let mut issuer: Option<String> = None;

    for param in query.split('&') {
        if let Some((key, value)) = param.split_once('=') {
            match key.to_lowercase().as_str() {
                "secret" => secret = Some(value.to_string()),
                "issuer" => issuer = Some(uri_decode(value)),
                _ => {}
            }
        }
    }

    Some((label, secret?, issuer))
}

/// Minimal percent-encoding for URI components.
fn uri_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9'
            | b'-' | b'_' | b'.' | b'~' | b':' | b'@' => out.push(b as char),
            _ => {
                out.push('%');
                out.push_str(&format!("{:02X}", b));
            }
        }
    }
    out
}

/// Percent-decoding on bytes: `%` and two hex digits give one byte, anything
/// else is kept as it is. Works on bytes, never on `str` offsets, so a
/// multi-byte character after `%` cannot split a char (it panicked, issue
/// #23 pass 2).
fn uri_decode_bytes(s: &str) -> Vec<u8> {
    fn hex(b: u8) -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        }
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(h << 4 | l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

/// Minimal percent-decoding of a label or an issuer (display only: an
/// invalid UTF-8 sequence shows as U+FFFD). The secret goes through
/// `uri_decode_bytes` and refuses it instead.
fn uri_decode(s: &str) -> String {
    String::from_utf8_lossy(&uri_decode_bytes(s)).to_string()
}

// ---------------------------------------------------------------------------
// Secret typed or pasted by the user (issue #23)
// ---------------------------------------------------------------------------

/// Why a secret given when adding a TOTP key is refused.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SecretInputError {
    /// Nothing but spaces.
    Empty,
    /// Not Base32 (RFC 4648 alphabet), or no data once decoded.
    InvalidBase32,
    /// A Base32 length no byte count gives (8k+1, 8k+3 or 8k+6 characters
    /// once the padding is removed): a truncated or doubled paste.
    InvalidLength,
    /// Under `MIN_SECRET_BYTES` (80 bits).
    TooShort,
    /// The same text twice in a row: a paste done twice.
    DoubledPaste,
    /// A link with the same parameter twice (two `secret=`): which one the
    /// site meant cannot be known.
    DuplicateParameter,
    /// An `otpauth://` link that is not `otpauth://totp/` (e.g. `hotp`), or
    /// another scheme.
    NotTotpUri,
    /// An `otpauth://totp/` link without a `secret=` parameter.
    UriWithoutSecret,
    /// The link asks for an algorithm, a number of digits or a period other
    /// than SHA1 / 6 / 30 s, the only ones `generate_code` computes: the codes
    /// would never match the site's.
    UnsupportedParameters,
}

/// The Base32 secret to store for what the user pasted when adding a key:
/// Base32 text (spaces, lowercase and `=` padding accepted) or an
/// `otpauth://totp/` link (its `secret`). Returns the secret uppercase,
/// without spaces or padding, or why it is refused. Nothing is guessed: an
/// input that is not exactly one of these forms is refused.
pub fn parse_secret_input(input: &str) -> Result<String, SecretInputError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(SecretInputError::Empty);
    }
    if is_doubled(input) {
        return Err(SecretInputError::DoubledPaste);
    }
    const PREFIX: &str = "otpauth://totp/";
    // `get`, not `[..n]`: a pasted text may have a multi-byte character there.
    let starts_with = |p: &str| input.get(..p.len()).map_or(false, |s| s.eq_ignore_ascii_case(p));
    if starts_with("otpauth:") {
        let is_totp = starts_with(PREFIX);
        if !is_totp {
            return Err(SecretInputError::NotTotpUri);
        }
        // `parse_uri` expects the scheme in lowercase.
        let uri = format!("{}{}", PREFIX, &input[PREFIX.len()..]);
        let (_label, secret, _issuer) = parse_uri(&uri).ok_or(SecretInputError::UriWithoutSecret)?;
        let query = uri.split_once('?').map(|(_, q)| q).unwrap_or("");
        let mut seen: Vec<String> = Vec::new();
        for param in query.split('&') {
            let Some((key, value)) = param.split_once('=') else { continue };
            // Compared once percent-decoded: "%53HA256" is SHA256.
            let key = String::from_utf8_lossy(&uri_decode_bytes(key)).to_ascii_lowercase();
            let value = String::from_utf8_lossy(&uri_decode_bytes(value)).to_string();
            // parse_uri keeps the last `secret=`: two of them (or of any
            // parameter) leave the site's intent unknown.
            if seen.contains(&key) {
                return Err(SecretInputError::DuplicateParameter);
            }
            seen.push(key.clone());
            let supported = match key.as_str() {
                "algorithm" => value.eq_ignore_ascii_case("SHA1"),
                "digits" => value == TOTP_DIGITS.to_string(),
                "period" => value == TOTP_PERIOD.to_string(),
                _ => true,
            };
            if !supported {
                return Err(SecretInputError::UnsupportedParameters);
            }
        }
        // Not UTF-8 once decoded: refused, never replaced by U+FFFD.
        let decoded = String::from_utf8(uri_decode_bytes(&secret))
            .map_err(|_| SecretInputError::InvalidBase32)?;
        return normalize_base32_secret(&decoded);
    }
    normalize_base32_secret(input)
}

/// An `otpauth://` link pasted twice: the same text twice in a row (spaces
/// and case ignored) holding the scheme. Base32 alone is not checked: a real
/// secret can have two equal halves (RFC 6238's own test secret,
/// "12345678901234567890", does), and the length check catches most
/// doubled Base32 pastes.
fn is_doubled(s: &str) -> bool {
    let clean: Vec<char> = s.chars().filter(|c| !c.is_whitespace()).flat_map(|c| c.to_uppercase()).collect();
    let n = clean.len();
    let half: String = clean[..n / 2].iter().collect();
    n >= 2 && n % 2 == 0 && clean[..n / 2] == clean[n / 2..] && half.starts_with("OTPAUTH:")
}

/// `s` without spaces or trailing `=`, uppercase, when it is Base32 holding
/// at least one byte.
fn normalize_base32_secret(s: &str) -> Result<String, SecretInputError> {
    let clean: String = s
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .trim_end_matches('=')
        .to_ascii_uppercase();
    if s.trim().is_empty() {
        return Err(SecretInputError::Empty);
    }
    if !clean.bytes().all(|b| BASE32_ALPHABET.contains(&b)) {
        return Err(SecretInputError::InvalidBase32);
    }
    if clean.is_empty() {
        return Err(SecretInputError::InvalidBase32);
    }
    // Base32 writes n bytes in ceil(8n/5) characters: 8k+1, 8k+3 and 8k+6
    // never occur, so such a text was cut or pasted twice.
    if matches!(clean.len() % 8, 1 | 3 | 6) {
        return Err(SecretInputError::InvalidLength);
    }
    match base32_decode(&clean) {
        Some(mut bytes) => {
            let len = bytes.len();
            bytes.zeroize();
            if len < MIN_SECRET_BYTES {
                Err(SecretInputError::TooShort)
            } else {
                Ok(clean)
            }
        }
        None => Err(SecretInputError::InvalidBase32),
    }
}

// ---------------------------------------------------------------------------
// Entry serialization
// ---------------------------------------------------------------------------

/// Serialize a `TotpEntry` to `"Name|SECRET"`.
pub fn serialize_entry(entry: &TotpEntry) -> String {
    format!("{}|{}", entry.name, entry.secret)
}

/// Parse `"Name|SECRET"` into a `TotpEntry`.
pub fn parse_entry(line: &str) -> Option<TotpEntry> {
    let (name, secret) = line.split_once('|')?;
    if name.is_empty() || secret.is_empty() {
        return None;
    }
    Some(TotpEntry {
        name: name.to_string(),
        secret: secret.to_string(),
    })
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -- Base32 encode/decode -----------------------------------------------

    #[test]
    fn base32_encode_empty() {
        assert_eq!(base32_encode(&[]), "");
    }

    #[test]
    fn base32_decode_empty() {
        assert_eq!(base32_decode("").unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn base32_decode_only_padding() {
        assert_eq!(base32_decode("====").unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn base32_roundtrip_hello() {
        let data = b"Hello";
        let encoded = base32_encode(data);
        assert_eq!(encoded, "JBSWY3DP");
        let decoded = base32_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn base32_roundtrip_arbitrary() {
        let data: Vec<u8> = (0..=255).collect();
        let encoded = base32_encode(&data);
        let decoded = base32_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn base32_decode_lowercase() {
        let decoded = base32_decode("jbswy3dp").unwrap();
        assert_eq!(decoded, b"Hello");
    }

    #[test]
    fn base32_decode_with_padding() {
        let decoded = base32_decode("JBSWY3DP======").unwrap();
        assert_eq!(decoded, b"Hello");
    }

    #[test]
    fn base32_decode_invalid_char() {
        assert!(base32_decode("JBSWY3DP!").is_none());
    }

    #[test]
    fn base32_rfc6238_secret() {
        // "12345678901234567890" → GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ
        let secret = b"12345678901234567890";
        let encoded = base32_encode(secret);
        assert_eq!(encoded, "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ");
        let decoded = base32_decode(&encoded).unwrap();
        assert_eq!(decoded, secret);
    }

    // -- RFC 6238 test vectors (SHA1) ---------------------------------------

    const TEST_SECRET_B32: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    #[test]
    fn rfc6238_time_59() {
        assert_eq!(
            generate_code(TEST_SECRET_B32, 59).unwrap(),
            "287082"
        );
    }

    #[test]
    fn rfc6238_time_1111111109() {
        assert_eq!(
            generate_code(TEST_SECRET_B32, 1111111109).unwrap(),
            "081804"
        );
    }

    #[test]
    fn rfc6238_time_1111111111() {
        assert_eq!(
            generate_code(TEST_SECRET_B32, 1111111111).unwrap(),
            "050471"
        );
    }

    #[test]
    fn rfc6238_time_1234567890() {
        assert_eq!(
            generate_code(TEST_SECRET_B32, 1234567890).unwrap(),
            "005924"
        );
    }

    #[test]
    fn rfc6238_time_2000000000() {
        assert_eq!(
            generate_code(TEST_SECRET_B32, 2000000000).unwrap(),
            "279037"
        );
    }

    // -- generate_code edge cases -------------------------------------------

    #[test]
    fn generate_code_empty_secret() {
        assert!(generate_code("", 59).is_none());
    }

    #[test]
    fn generate_code_invalid_base32() {
        assert!(generate_code("!!!!", 59).is_none());
    }

    // -- verify with tolerance ----------------------------------------------

    #[test]
    fn verify_exact_match() {
        assert!(verify(TEST_SECRET_B32, "287082", 59));
    }

    #[test]
    fn verify_previous_period() {
        // Code for time=59 (counter=1), verified at time=89 (counter=2)
        // ±1 tolerance means counter 1 is checked
        assert!(verify(TEST_SECRET_B32, "287082", 89));
    }

    #[test]
    fn verify_next_period() {
        // Code for time=59 (counter=1), verified at time=29 (counter=0)
        // ±1 tolerance means counter 1 is checked
        assert!(verify(TEST_SECRET_B32, "287082", 29));
    }

    #[test]
    fn verify_wrong_code() {
        assert!(!verify(TEST_SECRET_B32, "000000", 59));
    }

    #[test]
    fn verify_out_of_tolerance() {
        // Code for time=59 (counter=1), verified at time=150 (counter=5)
        // Way outside ±1
        assert!(!verify(TEST_SECRET_B32, "287082", 150));
    }

    #[test]
    fn verify_empty_secret() {
        assert!(!verify("", "287082", 59));
    }

    // -- remaining_seconds --------------------------------------------------

    #[test]
    fn remaining_seconds_in_range() {
        let r = remaining_seconds();
        assert!(r >= 1 && r <= 30, "remaining_seconds={} out of [1,30]", r);
    }

    // -- Secret generation --------------------------------------------------

    #[test]
    fn generate_secret_valid_base32() {
        let secret = generate_secret();
        let decoded = base32_decode(&secret).unwrap();
        assert_eq!(decoded.len(), TOTP_SECRET_SIZE);
    }

    #[test]
    fn generate_secret_unique() {
        let a = generate_secret();
        let b = generate_secret();
        assert_ne!(a, b);
    }

    // -- URI build/parse ----------------------------------------------------

    #[test]
    fn uri_roundtrip_with_issuer() {
        let uri = build_uri("user@example.com", "JBSWY3DPEHPK3PXP", Some("MyApp"));
        let (label, secret, issuer) = parse_uri(&uri).unwrap();
        assert_eq!(label, "user@example.com");
        assert_eq!(secret, "JBSWY3DPEHPK3PXP");
        assert_eq!(issuer.unwrap(), "MyApp");
    }

    #[test]
    fn uri_roundtrip_without_issuer() {
        let uri = build_uri("alice", "JBSWY3DP", None);
        let (label, secret, issuer) = parse_uri(&uri).unwrap();
        assert_eq!(label, "alice");
        assert_eq!(secret, "JBSWY3DP");
        assert!(issuer.is_none());
    }

    #[test]
    fn uri_with_issuer_prefix() {
        let uri = "otpauth://totp/MyIssuer:user@example.com?secret=JBSWY3DP&issuer=MyIssuer";
        let (label, secret, issuer) = parse_uri(uri).unwrap();
        assert_eq!(label, "MyIssuer:user@example.com");
        assert_eq!(secret, "JBSWY3DP");
        assert_eq!(issuer.unwrap(), "MyIssuer");
    }

    #[test]
    fn uri_parse_invalid_scheme() {
        assert!(parse_uri("otpauth://hotp/test?secret=ABC").is_none());
    }

    #[test]
    fn uri_parse_no_secret() {
        assert!(parse_uri("otpauth://totp/test?issuer=X").is_none());
    }

    #[test]
    fn uri_with_spaces() {
        let uri = build_uri("My App:user name", "JBSWY3DP", Some("My App"));
        let (label, secret, issuer) = parse_uri(&uri).unwrap();
        assert_eq!(label, "My App:user name");
        assert_eq!(secret, "JBSWY3DP");
        assert_eq!(issuer.unwrap(), "My App");
    }

    // -- parse_secret_input (issue #23) -------------------------------------

    #[test]
    fn pasted_base32_with_spaces_and_lowercase_is_accepted() {
        let s = parse_secret_input("  gezd gnbv gy3t qojq gezd gnbv gy3t qojq ").unwrap();
        assert_eq!(s, TEST_SECRET_B32);
        // Padding and tabs too.
        assert_eq!(parse_secret_input("JBSW\tY3DPEHPK3PXP====").unwrap(), "JBSWY3DPEHPK3PXP");
    }

    #[test]
    fn pasted_otpauth_uri_gives_its_secret() {
        let uri = "otpauth://totp/Example:alice@example.com?secret=gezdgnbvgy3tqojqgezdgnbvgy3tqojq&issuer=Example";
        assert_eq!(parse_secret_input(uri).unwrap(), TEST_SECRET_B32);
        // The defaults written out are accepted.
        let uri = "otpauth://totp/x?secret=JBSWY3DPEHPK3PXP&algorithm=SHA1&digits=6&period=30";
        assert_eq!(parse_secret_input(uri).unwrap(), "JBSWY3DPEHPK3PXP");
        // Percent-encoded spaces in the secret, as some sites write it.
        assert_eq!(parse_secret_input("otpauth://totp/x?secret=JBSW%20Y3DPEHPK3PXP").unwrap(), "JBSWY3DPEHPK3PXP");
    }

    #[test]
    fn stored_secret_gives_the_rfc6238_codes() {
        for input in [
            "gezd gnbv gy3t qojq gezd gnbv gy3t qojq",
            "otpauth://totp/RFC?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ",
        ] {
            let s = parse_secret_input(input).unwrap();
            assert_eq!(generate_code(&s, 59).unwrap(), "287082");
            assert_eq!(generate_code(&s, 1111111109).unwrap(), "081804");
            assert_eq!(generate_code(&s, 2000000000).unwrap(), "279037");
        }
    }

    #[test]
    fn refused_secrets_say_why() {
        use SecretInputError::*;
        assert_eq!(parse_secret_input(""), Err(Empty));
        assert_eq!(parse_secret_input("   "), Err(Empty));
        assert_eq!(parse_secret_input("JBSWY3DPEHPK3PXP1"), Err(InvalidBase32)); // 1 is not Base32
        assert_eq!(parse_secret_input("hello world!"), Err(InvalidBase32));
        assert_eq!(parse_secret_input("===="), Err(InvalidBase32));
        // Multi-byte characters: refused, never a panic on a char boundary.
        assert_eq!(parse_secret_input("ééééééééé"), Err(InvalidBase32));
        assert_eq!(parse_secret_input("otpaut\u{e9}://totp/x?secret=JBSWY3DPEHPK3PXP"), Err(InvalidBase32));
        assert_eq!(parse_secret_input("otpauth://tot\u{e9}/x?secret=JBSWY3DPEHPK3PXP"), Err(NotTotpUri));
        assert_eq!(parse_secret_input("otpauth://hotp/x?secret=JBSWY3DPEHPK3PXP&counter=1"), Err(NotTotpUri));
        assert_eq!(parse_secret_input("https://example.com/?secret=JBSWY3DPEHPK3PXP"), Err(InvalidBase32));
        assert_eq!(parse_secret_input("OTPAUTH://HOTP/x?secret=JBSWY3DPEHPK3PXP"), Err(NotTotpUri));
        assert_eq!(parse_secret_input("otpauth://totp/x?issuer=X"), Err(UriWithoutSecret));
        assert_eq!(parse_secret_input("otpauth://totp/x"), Err(UriWithoutSecret));
        assert_eq!(parse_secret_input("otpauth://totp/x?secret=JBSWY3D1"), Err(InvalidBase32));
        assert_eq!(parse_secret_input("otpauth://totp/x?secret=JBSWY3DPEHPK3PXP&algorithm=SHA256"), Err(UnsupportedParameters));
        assert_eq!(parse_secret_input("otpauth://totp/x?secret=JBSWY3DPEHPK3PXP&digits=8"), Err(UnsupportedParameters));
        assert_eq!(parse_secret_input("otpauth://totp/x?secret=JBSWY3DPEHPK3PXP&period=60"), Err(UnsupportedParameters));
    }

    /// Pass 2, B1: the percent-decoding sliced the text on byte offsets and
    /// panicked on a multi-byte character after `%` (with panic=abort, the
    /// whole app on a keystroke of the secret field).
    #[test]
    fn percent_sequences_never_panic_and_are_explicit() {
        use SecretInputError::*;
        assert_eq!(parse_secret_input("otpauth://totp/x?secret=JBSW%1\u{e9}"), Err(InvalidBase32));
        assert_eq!(parse_secret_input("otpauth://totp/x?secret=JBSW%a\u{e9}"), Err(InvalidBase32));
        assert_eq!(parse_secret_input("otpauth://totp/%1\u{e9}?secret=JBSWY3DPEHPK3PXP"), Ok("JBSWY3DPEHPK3PXP".into()));
        // A trailing escape is decoded (%41 = A).
        assert_eq!(parse_secret_input("otpauth://totp/x?secret=JBSWY3DPEHPK3PXP%41%41"), Ok("JBSWY3DPEHPK3PXPAA".into()));
        // %FF is not UTF-8 once decoded: refused, not replaced by U+FFFD.
        assert_eq!(parse_secret_input("otpauth://totp/x?secret=JBSWY3DPEHPK3PXP%FF"), Err(InvalidBase32));
        // `+` is not a hex digit (from_str_radix accepted "+1").
        assert_eq!(parse_secret_input("otpauth://totp/x?secret=JBSWY3DPEHPK3PXP%+1"), Err(InvalidBase32));
        // An incomplete escape stays as text, which is not Base32.
        assert_eq!(parse_secret_input("otpauth://totp/x?secret=JBSWY3DPEHPK3PXP%4"), Err(InvalidBase32));
        assert_eq!(uri_decode_bytes("%4"), b"%4".to_vec());
        assert_eq!(uri_decode_bytes("a%41"), b"aA".to_vec());
    }

    /// Fuzz-style: every short string over a hostile alphabet goes through
    /// without a panic (the field is parsed on each keystroke).
    #[test]
    fn parse_secret_input_never_panics() {
        let alphabet = ["%", "1", "a", "F", "\u{e9}", "\u{1F600}", "=", " ", "?", "&", "otpauth://totp/", "secret=", "OTPAUTH:", "A", "\u{0}"];
        let n = alphabet.len();
        let mut count = 0;
        for a in 0..n {
            for b in 0..n {
                for c in 0..n {
                    for d in 0..n {
                        let s = format!("{}{}{}{}", alphabet[a], alphabet[b], alphabet[c], alphabet[d]);
                        let _ = parse_secret_input(&s);
                        let _ = parse_secret_input(&format!("otpauth://totp/x?secret={}", s));
                        let _ = parse_uri(&format!("otpauth://totp/{}?secret=A", s));
                        count += 1;
                    }
                }
            }
        }
        assert_eq!(count, n * n * n * n);
    }

    /// Pass 2, M2: a truncated or concatenated paste must not be stored as
    /// a valid secret. A Base32 length that no byte count gives (8k+1, 8k+3,
    /// 8k+6 characters) and secrets under 80 bits are refused.
    #[test]
    fn truncated_or_short_secrets_are_refused() {
        use SecretInputError::*;
        assert_eq!(parse_secret_input("AAA"), Err(InvalidLength));
        assert_eq!(parse_secret_input("JBSWY3DPEHPK3PXPA"), Err(InvalidLength)); // 17 = 8*2+1
        assert_eq!(parse_secret_input("JBSWY3DPEHPK3P"), Err(InvalidLength)); // 14 = 8+6
        // 15 characters: 9 bytes, under the 10-byte floor.
        assert_eq!(parse_secret_input("JBSWY3DPEHPK3PX"), Err(TooShort));
        // A 5-byte secret.
        assert_eq!(parse_secret_input("JBSWY3DP"), Err(TooShort));
        assert_eq!(parse_secret_input("otpauth://totp/x?secret=JBSWY3DP"), Err(TooShort));
        // 10 bytes (16 characters) and the 20-byte RFC 6238 secret pass.
        assert_eq!(parse_secret_input("JBSWY3DPEHPK3PXP").unwrap(), "JBSWY3DPEHPK3PXP");
        assert_eq!(parse_secret_input(TEST_SECRET_B32).unwrap(), TEST_SECRET_B32);
    }

    /// Pass 3: two `secret=` are refused (not last-wins), the parameters are
    /// compared once percent-decoded, and a paste done twice is named.
    #[test]
    fn ambiguous_links_and_doubled_pastes_are_refused() {
        use SecretInputError::*;
        let s = "JBSWY3DPEHPK3PXP";
        assert_eq!(parse_secret_input(&format!("otpauth://totp/x?secret={s}&secret=GEZDGNBVGY3TQOJQ")), Err(DuplicateParameter));
        assert_eq!(parse_secret_input(&format!("otpauth://totp/x?secret={s}&digits=6&DIGITS=8")), Err(DuplicateParameter));
        // %53HA256 is SHA256 once decoded; %36 is 6.
        assert_eq!(parse_secret_input(&format!("otpauth://totp/x?secret={s}&algorithm=%53HA256")), Err(UnsupportedParameters));
        assert_eq!(parse_secret_input(&format!("otpauth://totp/x?secret={s}&digits=%36")), Ok(s.to_string()));
        assert_eq!(parse_secret_input(&format!("otpauth://totp/x?secret={s}&%64igits=8")), Err(UnsupportedParameters));
        // Base32 twice cannot be told from a real secret with equal halves
        // (the RFC 6238 one below): accepted; a link twice is refused.
        assert_eq!(parse_secret_input(&format!("{s}{s}")), Ok(format!("{s}{s}")));
        let uri = format!("otpauth://totp/x?secret={s}");
        assert_eq!(parse_secret_input(&format!("{uri}{uri}")), Err(DoubledPaste));
        // A real secret is still accepted.
        assert_eq!(parse_secret_input(TEST_SECRET_B32).unwrap(), TEST_SECRET_B32);
    }

    // -- Entry serialization ------------------------------------------------

    #[test]
    fn entry_roundtrip() {
        let entry = TotpEntry {
            name: "GitHub".to_string(),
            secret: "JBSWY3DPEHPK3PXP".to_string(),
        };
        let line = serialize_entry(&entry);
        assert_eq!(line, "GitHub|JBSWY3DPEHPK3PXP");
        let parsed = parse_entry(&line).unwrap();
        assert_eq!(parsed, entry);
    }

    #[test]
    fn entry_parse_empty_name() {
        assert!(parse_entry("|SECRET").is_none());
    }

    #[test]
    fn entry_parse_empty_secret() {
        assert!(parse_entry("Name|").is_none());
    }

    #[test]
    fn entry_parse_no_pipe() {
        assert!(parse_entry("NoSeparator").is_none());
    }

    #[test]
    fn entry_with_pipe_in_name() {
        // split_once splits on the first pipe only
        let parsed = parse_entry("My|Service|SECRET").unwrap();
        assert_eq!(parsed.name, "My");
        assert_eq!(parsed.secret, "Service|SECRET");
    }
}
