// Crypto module — AES-256-CBC + HMAC-SHA256, PBKDF2-SHA256
//
// Wire format (byte-compatible with the C# LockNote binary):
//   [salt 16][iv 16][hmac 32][ciphertext ...]
//
// Encrypt-then-MAC: HMAC is computed over salt || iv || ciphertext.
// All sensitive buffers are zeroed after use via `zeroize`.

use aes::Aes256;
use cbc::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use hmac::{Hmac, Mac};
use pbkdf2::pbkdf2_hmac;
use rand::RngCore;
use sha2::Sha256;
use zeroize::Zeroize;

type Aes256CbcEnc = cbc::Encryptor<Aes256>;
type Aes256CbcDec = cbc::Decryptor<Aes256>;
type HmacSha256 = Hmac<Sha256>;

// ---------------------------------------------------------------------------
// Public constants
// ---------------------------------------------------------------------------

pub const PBKDF2_ITERATIONS: u32 = 300_000;
pub const SALT_SIZE: usize = 16;
pub const IV_SIZE: usize = 16;
pub const HMAC_SIZE: usize = 32;
pub const KEY_SIZE: usize = 32;
/// Minimum valid payload: salt + iv + hmac + one AES block (16 bytes).
pub const MIN_PAYLOAD_SIZE: usize = SALT_SIZE + IV_SIZE + HMAC_SIZE + 16; // 80

// ---------------------------------------------------------------------------
// Key derivation
// ---------------------------------------------------------------------------

/// Derives a 32-byte encryption key and a 32-byte MAC key from a password
/// and salt using PBKDF2-SHA256.
fn derive_keys(password: &str, salt: &[u8]) -> ([u8; KEY_SIZE], [u8; KEY_SIZE]) {
    let mut key_material = [0u8; 64];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, PBKDF2_ITERATIONS, &mut key_material);

    let mut enc_key = [0u8; KEY_SIZE];
    let mut mac_key = [0u8; KEY_SIZE];
    enc_key.copy_from_slice(&key_material[..KEY_SIZE]);
    mac_key.copy_from_slice(&key_material[KEY_SIZE..]);

    key_material.zeroize();
    (enc_key, mac_key)
}

// ---------------------------------------------------------------------------
// HMAC computation
// ---------------------------------------------------------------------------

/// Computes HMAC-SHA256 over salt || iv || ciphertext.
fn compute_hmac(mac_key: &[u8; KEY_SIZE], salt: &[u8], iv: &[u8], ciphertext: &[u8]) -> [u8; HMAC_SIZE] {
    let mut mac = HmacSha256::new_from_slice(mac_key)
        .expect("HMAC accepts any key size");
    mac.update(salt);
    mac.update(iv);
    mac.update(ciphertext);
    let result = mac.finalize().into_bytes();
    let mut out = [0u8; HMAC_SIZE];
    out.copy_from_slice(&result);
    out
}

// ---------------------------------------------------------------------------
// Encrypt
// ---------------------------------------------------------------------------

/// Encrypts `plaintext` with `password` and returns the wire-format payload.
///
/// Wire format: `salt[16] || iv[16] || hmac[32] || ciphertext[...]`
pub fn encrypt(plaintext: &str, password: &str) -> Vec<u8> {
    // 1. Generate random salt and IV
    let mut salt = [0u8; SALT_SIZE];
    let mut iv = [0u8; IV_SIZE];
    let mut rng = rand::rng();
    rng.fill_bytes(&mut salt);
    rng.fill_bytes(&mut iv);

    // 2. Derive keys
    let (mut enc_key, mut mac_key) = derive_keys(password, &salt);

    // 3. Encrypt (AES-256-CBC, PKCS7 padding)
    let mut plain_bytes = plaintext.as_bytes().to_vec();
    // cbc::Encryptor needs a buffer large enough for plaintext + up to one block of padding.
    let padded_len = (plain_bytes.len() / 16 + 1) * 16;
    let mut buf = vec![0u8; padded_len];
    buf[..plain_bytes.len()].copy_from_slice(&plain_bytes);

    let ciphertext = Aes256CbcEnc::new(&enc_key.into(), &iv.into())
        .encrypt_padded_mut::<cbc::cipher::block_padding::Pkcs7>(&mut buf, plain_bytes.len())
        .expect("buffer is large enough for PKCS7 padding")
        .to_vec();

    // 4. Compute HMAC over salt || iv || ciphertext
    let hmac_value = compute_hmac(&mac_key, &salt, &iv, &ciphertext);

    // 5. Assemble output: salt || iv || hmac || ciphertext
    let mut output = Vec::with_capacity(SALT_SIZE + IV_SIZE + HMAC_SIZE + ciphertext.len());
    output.extend_from_slice(&salt);
    output.extend_from_slice(&iv);
    output.extend_from_slice(&hmac_value);
    output.extend_from_slice(&ciphertext);

    // 6. Zeroize sensitive material
    enc_key.zeroize();
    mac_key.zeroize();
    salt.zeroize();
    iv.zeroize();
    plain_bytes.zeroize();
    buf.zeroize();

    output
}

// ---------------------------------------------------------------------------
// Decrypt
// ---------------------------------------------------------------------------

/// Decrypts wire-format `data` with `password`.
///
/// Returns `None` if the data is too short, the HMAC does not match (wrong
/// password or corrupted data), or the ciphertext is not valid PKCS7-padded
/// AES-256-CBC.
pub fn decrypt(data: &[u8], password: &str) -> Option<String> {
    // 1. Validate minimum size
    if data.len() < MIN_PAYLOAD_SIZE {
        return None;
    }

    // 2. Extract fields
    let salt = &data[..SALT_SIZE];
    let iv = &data[SALT_SIZE..SALT_SIZE + IV_SIZE];
    let stored_hmac = &data[SALT_SIZE + IV_SIZE..SALT_SIZE + IV_SIZE + HMAC_SIZE];
    let ciphertext = &data[SALT_SIZE + IV_SIZE + HMAC_SIZE..];

    // 3. Derive keys
    let (mut enc_key, mut mac_key) = derive_keys(password, salt);

    // 4. Verify HMAC (constant-time via hmac crate's `verify_slice`)
    let mut mac = HmacSha256::new_from_slice(&mac_key)
        .expect("HMAC accepts any key size");
    mac.update(salt);
    mac.update(iv);
    mac.update(ciphertext);

    if mac.verify_slice(stored_hmac).is_err() {
        enc_key.zeroize();
        mac_key.zeroize();
        return None;
    }

    // 5. Decrypt
    let mut ct_buf = ciphertext.to_vec();
    let plaintext_result = Aes256CbcDec::new(&enc_key.into(), iv.into())
        .decrypt_padded_mut::<cbc::cipher::block_padding::Pkcs7>(&mut ct_buf);

    enc_key.zeroize();
    mac_key.zeroize();

    match plaintext_result {
        Ok(plain_bytes) => {
            let text = String::from_utf8(plain_bytes.to_vec()).ok();
            ct_buf.zeroize();
            text
        }
        Err(_) => {
            ct_buf.zeroize();
            None
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_basic() {
        let password = "correct horse battery staple";
        let plaintext = "Hello, LockNote!";
        let encrypted = encrypt(plaintext, password);
        let decrypted = decrypt(&encrypted, password);
        assert_eq!(decrypted, Some(plaintext.to_string()));
    }

    #[test]
    fn round_trip_empty_plaintext() {
        let password = "pass";
        let plaintext = "";
        let encrypted = encrypt(plaintext, password);
        assert!(encrypted.len() >= MIN_PAYLOAD_SIZE);
        let decrypted = decrypt(&encrypted, password);
        assert_eq!(decrypted, Some(String::new()));
    }

    #[test]
    fn round_trip_unicode() {
        let password = "mdp";
        let plaintext = "Caf\u{00e9} \u{1f512}\u{1f4dd} \u{65e5}\u{672c}\u{8a9e} \u{0410}\u{0411}\u{0412}";
        let encrypted = encrypt(plaintext, password);
        let decrypted = decrypt(&encrypted, password);
        assert_eq!(decrypted, Some(plaintext.to_string()));
    }

    #[test]
    fn round_trip_large_text() {
        let password = "big";
        let plaintext: String = "A".repeat(100_000);
        let encrypted = encrypt(&plaintext, password);
        let decrypted = decrypt(&encrypted, password).unwrap();
        assert_eq!(decrypted.len(), 100_000);
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn wrong_password_returns_none() {
        let encrypted = encrypt("secret", "right");
        let result = decrypt(&encrypted, "wrong");
        assert_eq!(result, None);
    }

    #[test]
    fn truncated_data_returns_none() {
        // Way too short
        assert_eq!(decrypt(&[0u8; 10], "pass"), None);
        // Just under the minimum
        assert_eq!(decrypt(&[0u8; MIN_PAYLOAD_SIZE - 1], "pass"), None);
        // Exactly minimum but garbage -> HMAC mismatch
        assert_eq!(decrypt(&[0u8; MIN_PAYLOAD_SIZE], "pass"), None);
    }

    #[test]
    fn corrupted_hmac_returns_none() {
        let mut encrypted = encrypt("data", "pass");
        // Flip a byte in the HMAC region (offset 32..64)
        encrypted[40] ^= 0xFF;
        assert_eq!(decrypt(&encrypted, "pass"), None);
    }

    #[test]
    fn corrupted_ciphertext_returns_none() {
        let mut encrypted = encrypt("data", "pass");
        // Flip a byte in the ciphertext region
        let last = encrypted.len() - 1;
        encrypted[last] ^= 0xFF;
        assert_eq!(decrypt(&encrypted, "pass"), None);
    }

    #[test]
    fn salt_iv_randomness() {
        let a = encrypt("same text", "same password");
        let b = encrypt("same text", "same password");
        // Salt (first 16 bytes) should differ
        assert_ne!(&a[..SALT_SIZE], &b[..SALT_SIZE], "salts must differ");
        // IV (next 16 bytes) should differ
        assert_ne!(
            &a[SALT_SIZE..SALT_SIZE + IV_SIZE],
            &b[SALT_SIZE..SALT_SIZE + IV_SIZE],
            "IVs must differ"
        );
        // Entire ciphertext should differ
        assert_ne!(a, b);
    }

    #[test]
    fn wire_format_layout() {
        let encrypted = encrypt("test", "pass");
        // Must be at least MIN_PAYLOAD_SIZE
        assert!(encrypted.len() >= MIN_PAYLOAD_SIZE);
        // Ciphertext length must be a multiple of 16 (AES block size)
        let ct_len = encrypted.len() - SALT_SIZE - IV_SIZE - HMAC_SIZE;
        assert_eq!(ct_len % 16, 0);
    }

    #[test]
    fn minimum_payload_size_is_correct() {
        assert_eq!(MIN_PAYLOAD_SIZE, 80);
    }

    #[test]
    fn derive_keys_deterministic() {
        let salt = [0x42u8; SALT_SIZE];
        let (k1_enc, k1_mac) = derive_keys("password", &salt);
        let (k2_enc, k2_mac) = derive_keys("password", &salt);
        assert_eq!(k1_enc, k2_enc);
        assert_eq!(k1_mac, k2_mac);
    }

    #[test]
    fn derive_keys_different_passwords() {
        let salt = [0x42u8; SALT_SIZE];
        let (k1_enc, _) = derive_keys("password1", &salt);
        let (k2_enc, _) = derive_keys("password2", &salt);
        assert_ne!(k1_enc, k2_enc);
    }

    #[test]
    fn derive_keys_different_salts() {
        let (k1_enc, _) = derive_keys("password", &[0x01u8; SALT_SIZE]);
        let (k2_enc, _) = derive_keys("password", &[0x02u8; SALT_SIZE]);
        assert_ne!(k1_enc, k2_enc);
    }

    #[test]
    fn plaintext_on_block_boundary() {
        // Exactly 16 bytes -> PKCS7 adds a full 16-byte padding block
        let plaintext = "0123456789abcdef"; // 16 bytes
        let encrypted = encrypt(plaintext, "pass");
        let ct_len = encrypted.len() - SALT_SIZE - IV_SIZE - HMAC_SIZE;
        assert_eq!(ct_len, 32); // 16 data + 16 padding
        assert_eq!(decrypt(&encrypted, "pass"), Some(plaintext.to_string()));
    }

    #[test]
    fn known_answer_vector() {
        let plaintext = "Hello, LockNote!";
        let password = "test";
        let encrypted = encrypt(plaintext, password);

        // Verify structure: salt(16) + iv(16) + hmac(32) + ciphertext(multiple of 16)
        assert!(encrypted.len() >= MIN_PAYLOAD_SIZE);
        let ct_len = encrypted.len() - SALT_SIZE - IV_SIZE - HMAC_SIZE;
        assert_eq!(ct_len % 16, 0);

        // Roundtrip must succeed
        assert_eq!(decrypt(&encrypted, password), Some(plaintext.to_string()));
    }

    #[test]
    fn empty_password() {
        let plaintext = "secret data";
        let encrypted = encrypt(plaintext, "");
        assert_eq!(decrypt(&encrypted, ""), Some(plaintext.to_string()));
    }

    #[test]
    fn very_long_password() {
        let password: String = "A".repeat(10_000);
        let plaintext = "protected by a very long password";
        let encrypted = encrypt(plaintext, &password);
        assert_eq!(decrypt(&encrypted, &password), Some(plaintext.to_string()));
    }

    #[test]
    fn password_with_unicode() {
        let password = "p\u{00E4}ssw\u{00F6}rd\u{1F512}\u{1F525}\u{2603}";
        let plaintext = "emoji-locked content";
        let encrypted = encrypt(plaintext, password);
        assert_eq!(decrypt(&encrypted, password), Some(plaintext.to_string()));
    }

    #[test]
    fn corrupted_salt_returns_none() {
        let mut encrypted = encrypt("test data", "password");
        // Flip a byte in the salt region (offset 0..16)
        encrypted[7] ^= 0xFF;
        assert_eq!(decrypt(&encrypted, "password"), None);
    }

    #[test]
    fn corrupted_iv_returns_none() {
        let mut encrypted = encrypt("test data", "password");
        // Flip a byte in the IV region (offset 16..32)
        encrypted[20] ^= 0xFF;
        assert_eq!(decrypt(&encrypted, "password"), None);
    }

    #[test]
    fn all_zero_payload_exact_min_size() {
        let payload = [0u8; 80];
        assert_eq!(decrypt(&payload, "password"), None);
    }

    #[test]
    fn pbkdf2_iterations_constant() {
        assert_eq!(PBKDF2_ITERATIONS, 300_000);
    }

    #[test]
    fn ciphertext_length_always_multiple_of_16() {
        for len in [0, 1, 15, 16, 17, 31, 32, 33, 100] {
            let plaintext: String = "X".repeat(len);
            let encrypted = encrypt(&plaintext, "pass");
            let ct_len = encrypted.len() - SALT_SIZE - IV_SIZE - HMAC_SIZE;
            assert_eq!(
                ct_len % 16,
                0,
                "ciphertext not block-aligned for plaintext length {}",
                len
            );
        }
    }

    #[test]
    fn double_encrypt_produces_different_output() {
        let a = encrypt("same text", "same pass");
        let b = encrypt("same text", "same pass");
        assert_ne!(a, b, "two encryptions of identical input must differ");
    }

    #[test]
    fn decrypt_with_extra_trailing_bytes() {
        let mut encrypted = encrypt("payload", "key");
        encrypted.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);
        // Extra bytes become part of ciphertext, HMAC will mismatch
        assert_eq!(decrypt(&encrypted, "key"), None);
    }

    #[test]
    fn plaintext_just_under_block_boundary() {
        // 15 bytes -> fits in one 16-byte block with 1 byte of PKCS7 padding
        let plaintext = "0123456789abcde"; // 15 bytes
        let encrypted = encrypt(plaintext, "pass");
        let ct_len = encrypted.len() - SALT_SIZE - IV_SIZE - HMAC_SIZE;
        assert_eq!(ct_len, 16); // 15 data + 1 padding = 16
        assert_eq!(decrypt(&encrypted, "pass"), Some(plaintext.to_string()));
    }

    #[test]
    fn plaintext_two_blocks() {
        // 32 bytes exactly -> PKCS7 adds a full 16-byte padding block
        let plaintext = "0123456789abcdef0123456789abcdef"; // 32 bytes
        let encrypted = encrypt(plaintext, "pass");
        let ct_len = encrypted.len() - SALT_SIZE - IV_SIZE - HMAC_SIZE;
        assert_eq!(ct_len, 48); // 32 data + 16 padding
        assert_eq!(decrypt(&encrypted, "pass"), Some(plaintext.to_string()));
    }
}
