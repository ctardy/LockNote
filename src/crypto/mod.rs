// Crypto module — AES-256-CBC + HMAC-SHA256, Argon2id
//
// Wire format:
//   [salt 16][iv 16][m_cost 4 LE][t_cost 4 LE][p_lanes 4 LE][hmac 32][ciphertext ...]
//   HMAC is computed over salt || iv || m_cost || t_cost || p_lanes || ciphertext.
//
// All sensitive buffers are zeroed after use via `zeroize`.

use aes::Aes256;
use argon2::Argon2;
use cbc::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::Sha256;
use zeroize::Zeroize;

type Aes256CbcEnc = cbc::Encryptor<Aes256>;
type Aes256CbcDec = cbc::Decryptor<Aes256>;
type HmacSha256 = Hmac<Sha256>;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors emitted by the crypto module.
///
/// The `HmacMismatch` variant is intentionally opaque (no inner detail)
/// because it is the typical signal for "wrong password" — callers must take
/// care not to leak distinguishing information to users (anti-oracle).
///
/// `CipherInit` is reserved for future key-size mismatches; today every
/// init path uses a fixed-size 32-byte key so the cipher constructors are
/// infallible, but we keep the variant for forward compatibility.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum CryptoError {
    InvalidParameters(String),
    KdfFailed(String),
    CipherInit(String),
    Encryption(String),
    Decryption(String),
    HmacMismatch,
    InvalidInput(String),
}

impl std::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidParameters(s) => write!(f, "Invalid crypto parameters: {}", s),
            Self::KdfFailed(s) => write!(f, "Key derivation failed: {}", s),
            Self::CipherInit(s) => write!(f, "Cipher init failed: {}", s),
            Self::Encryption(s) => write!(f, "Encryption failed: {}", s),
            Self::Decryption(s) => write!(f, "Decryption failed: {}", s),
            Self::HmacMismatch => write!(f, "HMAC verification failed"),
            Self::InvalidInput(s) => write!(f, "Invalid input: {}", s),
        }
    }
}

impl std::error::Error for CryptoError {}

// ---------------------------------------------------------------------------
// Public constants
// ---------------------------------------------------------------------------

/// Argon2id memory cost in KiB (64 MiB).
pub const DEFAULT_M_COST: u32 = 65_536;
/// Argon2id time cost (iterations).
pub const DEFAULT_T_COST: u32 = 3;
/// Argon2id parallelism (lanes).
pub const DEFAULT_P_LANES: u32 = 4;

pub const SALT_SIZE: usize = 16;
pub const IV_SIZE: usize = 16;
pub const ARGON2_PARAMS_SIZE: usize = 12; // m_cost(4) + t_cost(4) + p_lanes(4)
pub const HMAC_SIZE: usize = 32;
pub const KEY_SIZE: usize = 32;
/// Minimum valid payload: salt + iv + argon2_params + hmac + one AES block (16 bytes).
pub const MIN_PAYLOAD_SIZE: usize = SALT_SIZE + IV_SIZE + ARGON2_PARAMS_SIZE + HMAC_SIZE + 16; // 92

// ---------------------------------------------------------------------------
// Key derivation
// ---------------------------------------------------------------------------

/// Derives a 32-byte encryption key and a 32-byte MAC key from a password
/// and salt using Argon2id with the specified parameters.
fn derive_keys(
    password: &str,
    salt: &[u8],
    m_cost: u32,
    t_cost: u32,
    p_lanes: u32,
) -> Result<([u8; KEY_SIZE], [u8; KEY_SIZE]), CryptoError> {
    let mut key_material = [0u8; 64];

    let params = argon2::Params::new(m_cost, t_cost, p_lanes, Some(64))
        .map_err(|e| CryptoError::InvalidParameters(e.to_string()))?;
    let argon2 = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut key_material)
        .map_err(|e| CryptoError::KdfFailed(e.to_string()))?;

    let mut enc_key = [0u8; KEY_SIZE];
    let mut mac_key = [0u8; KEY_SIZE];
    enc_key.copy_from_slice(&key_material[..KEY_SIZE]);
    mac_key.copy_from_slice(&key_material[KEY_SIZE..]);

    key_material.zeroize();
    Ok((enc_key, mac_key))
}

// ---------------------------------------------------------------------------
// HMAC computation
// ---------------------------------------------------------------------------

/// Computes HMAC-SHA256 over salt || iv || argon2_params || ciphertext.
fn compute_hmac(
    mac_key: &[u8; KEY_SIZE],
    salt: &[u8],
    iv: &[u8],
    argon2_params: &[u8],
    ciphertext: &[u8],
) -> Result<[u8; HMAC_SIZE], CryptoError> {
    // HMAC-SHA256 accepts any key size; this branch is effectively unreachable
    // but we propagate a typed error to keep all cipher init paths fallible.
    let mut mac = HmacSha256::new_from_slice(mac_key)
        .map_err(|e| CryptoError::InvalidParameters(e.to_string()))?;
    mac.update(salt);
    mac.update(iv);
    mac.update(argon2_params);
    mac.update(ciphertext);
    let result = mac.finalize().into_bytes();
    let mut out = [0u8; HMAC_SIZE];
    out.copy_from_slice(&result);
    Ok(out)
}

// ---------------------------------------------------------------------------
// Encrypt
// ---------------------------------------------------------------------------

/// Encrypts `plaintext` with `password` using the default Argon2id parameters
/// and returns the wire-format payload.
///
/// Wire format: `salt[16] || iv[16] || m_cost[4 LE] || t_cost[4 LE] || p_lanes[4 LE] || hmac[32] || ciphertext[...]`
pub fn encrypt(plaintext: &str, password: &str) -> Result<Vec<u8>, CryptoError> {
    // 1. Generate random salt and IV
    let mut salt = [0u8; SALT_SIZE];
    let mut iv = [0u8; IV_SIZE];
    let mut rng = rand::rng();
    rng.fill_bytes(&mut salt);
    rng.fill_bytes(&mut iv);

    // 2. Derive keys (fixed Argon2id parameters)
    let (mut enc_key, mut mac_key) =
        derive_keys(password, &salt, DEFAULT_M_COST, DEFAULT_T_COST, DEFAULT_P_LANES)?;

    // 3. Encrypt (AES-256-CBC, PKCS7 padding)
    let mut plain_bytes = plaintext.as_bytes().to_vec();
    let padded_len = (plain_bytes.len() / 16 + 1) * 16;
    let mut buf = vec![0u8; padded_len];
    buf[..plain_bytes.len()].copy_from_slice(&plain_bytes);

    let ciphertext = Aes256CbcEnc::new(&enc_key.into(), &iv.into())
        .encrypt_padded_mut::<cbc::cipher::block_padding::Pkcs7>(&mut buf, plain_bytes.len())
        .map_err(|e| CryptoError::Encryption(e.to_string()))?
        .to_vec();

    // 4. Encode Argon2id parameters as 12 bytes little-endian
    let mut params_bytes = [0u8; ARGON2_PARAMS_SIZE];
    params_bytes[0..4].copy_from_slice(&DEFAULT_M_COST.to_le_bytes());
    params_bytes[4..8].copy_from_slice(&DEFAULT_T_COST.to_le_bytes());
    params_bytes[8..12].copy_from_slice(&DEFAULT_P_LANES.to_le_bytes());

    // 5. Compute HMAC over salt || iv || params || ciphertext
    let hmac_value = compute_hmac(&mac_key, &salt, &iv, &params_bytes, &ciphertext)?;

    // 6. Assemble output: salt || iv || params || hmac || ciphertext
    let mut output =
        Vec::with_capacity(SALT_SIZE + IV_SIZE + ARGON2_PARAMS_SIZE + HMAC_SIZE + ciphertext.len());
    output.extend_from_slice(&salt);
    output.extend_from_slice(&iv);
    output.extend_from_slice(&params_bytes);
    output.extend_from_slice(&hmac_value);
    output.extend_from_slice(&ciphertext);

    // 7. Zeroize sensitive material
    enc_key.zeroize();
    mac_key.zeroize();
    salt.zeroize();
    iv.zeroize();
    plain_bytes.zeroize();
    buf.zeroize();

    Ok(output)
}

// ---------------------------------------------------------------------------
// Decrypt
// ---------------------------------------------------------------------------

/// Decrypts wire-format `data` with `password`.
///
/// Returns `Err(CryptoError::HmacMismatch)` if the HMAC does not match (wrong
/// password or corrupted data). Other variants signal structural issues
/// (too-short input, invalid Argon2 parameters, decryption failure).
///
/// UI callers must avoid leaking the specific variant back to the user when
/// showing "Invalid password" — see `CryptoError::HmacMismatch`.
pub fn decrypt(data: &[u8], password: &str) -> Result<String, CryptoError> {
    if data.len() < MIN_PAYLOAD_SIZE {
        return Err(CryptoError::InvalidInput(format!(
            "payload too short ({} bytes, min {})",
            data.len(),
            MIN_PAYLOAD_SIZE
        )));
    }

    let salt = &data[..SALT_SIZE];
    let iv = &data[SALT_SIZE..SALT_SIZE + IV_SIZE];
    let params_bytes = &data[SALT_SIZE + IV_SIZE..SALT_SIZE + IV_SIZE + ARGON2_PARAMS_SIZE];
    let stored_hmac = &data[SALT_SIZE + IV_SIZE + ARGON2_PARAMS_SIZE
        ..SALT_SIZE + IV_SIZE + ARGON2_PARAMS_SIZE + HMAC_SIZE];
    let ciphertext = &data[SALT_SIZE + IV_SIZE + ARGON2_PARAMS_SIZE + HMAC_SIZE..];

    let m_cost = u32::from_le_bytes([params_bytes[0], params_bytes[1], params_bytes[2], params_bytes[3]]);
    let t_cost = u32::from_le_bytes([params_bytes[4], params_bytes[5], params_bytes[6], params_bytes[7]]);
    let p_lanes = u32::from_le_bytes([params_bytes[8], params_bytes[9], params_bytes[10], params_bytes[11]]);

    // Reject obviously invalid parameters
    if m_cost < 8 || t_cost == 0 || p_lanes == 0 {
        return Err(CryptoError::InvalidParameters(format!(
            "out-of-range Argon2 params (m={}, t={}, p={})",
            m_cost, t_cost, p_lanes
        )));
    }
    // Cap to prevent DoS: 1 GiB memory, 100 iterations, 255 lanes
    if m_cost > 1_048_576 || t_cost > 100 || p_lanes > 255 {
        return Err(CryptoError::InvalidParameters(format!(
            "Argon2 params exceed safety caps (m={}, t={}, p={})",
            m_cost, t_cost, p_lanes
        )));
    }

    // Derive keys with the stored parameters
    let (mut enc_key, mut mac_key) = derive_keys(password, salt, m_cost, t_cost, p_lanes)?;

    // Verify HMAC over salt || iv || params || ciphertext
    let mut mac = match HmacSha256::new_from_slice(&mac_key) {
        Ok(m) => m,
        Err(e) => {
            enc_key.zeroize();
            mac_key.zeroize();
            return Err(CryptoError::InvalidParameters(e.to_string()));
        }
    };
    mac.update(salt);
    mac.update(iv);
    mac.update(params_bytes);
    mac.update(ciphertext);

    if mac.verify_slice(stored_hmac).is_err() {
        enc_key.zeroize();
        mac_key.zeroize();
        return Err(CryptoError::HmacMismatch);
    }

    // Decrypt
    let mut ct_buf = ciphertext.to_vec();
    let plaintext_result = Aes256CbcDec::new(&enc_key.into(), iv.into())
        .decrypt_padded_mut::<cbc::cipher::block_padding::Pkcs7>(&mut ct_buf);

    enc_key.zeroize();
    mac_key.zeroize();

    match plaintext_result {
        Ok(plain_bytes) => {
            let owned = plain_bytes.to_vec();
            ct_buf.zeroize();
            String::from_utf8(owned)
                .map_err(|e| CryptoError::Decryption(format!("invalid UTF-8: {}", e)))
        }
        Err(e) => {
            ct_buf.zeroize();
            Err(CryptoError::Decryption(e.to_string()))
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
        let encrypted = encrypt(plaintext, password).unwrap();
        let decrypted = decrypt(&encrypted, password).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn round_trip_empty_plaintext() {
        let password = "pass";
        let plaintext = "";
        let encrypted = encrypt(plaintext, password).unwrap();
        assert!(encrypted.len() >= MIN_PAYLOAD_SIZE);
        let decrypted = decrypt(&encrypted, password).unwrap();
        assert_eq!(decrypted, String::new());
    }

    #[test]
    fn round_trip_unicode() {
        let password = "mdp";
        let plaintext = "Caf\u{00e9} \u{1f512}\u{1f4dd} \u{65e5}\u{672c}\u{8a9e} \u{0410}\u{0411}\u{0412}";
        let encrypted = encrypt(plaintext, password).unwrap();
        let decrypted = decrypt(&encrypted, password).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn round_trip_large_text() {
        let password = "big";
        let plaintext: String = "A".repeat(100_000);
        let encrypted = encrypt(&plaintext, password).unwrap();
        let decrypted = decrypt(&encrypted, password).unwrap();
        assert_eq!(decrypted.len(), 100_000);
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn wrong_password_returns_hmac_mismatch() {
        let encrypted = encrypt("secret", "right").unwrap();
        let result = decrypt(&encrypted, "wrong");
        assert!(matches!(result, Err(CryptoError::HmacMismatch)));
    }

    #[test]
    fn truncated_data_returns_err() {
        assert!(decrypt(&[0u8; 10], "pass").is_err());
        assert!(decrypt(&[0u8; MIN_PAYLOAD_SIZE - 1], "pass").is_err());
        // A zero-filled MIN_PAYLOAD_SIZE buffer has all-zero Argon2 params (m=0),
        // which is rejected as InvalidParameters.
        assert!(decrypt(&[0u8; MIN_PAYLOAD_SIZE], "pass").is_err());
    }

    #[test]
    fn corrupted_hmac_returns_err() {
        let mut encrypted = encrypt("data", "pass").unwrap();
        // Flip a byte in the HMAC region (offset 44..76)
        encrypted[50] ^= 0xFF;
        assert!(matches!(decrypt(&encrypted, "pass"), Err(CryptoError::HmacMismatch)));
    }

    #[test]
    fn corrupted_ciphertext_returns_err() {
        let mut encrypted = encrypt("data", "pass").unwrap();
        let last = encrypted.len() - 1;
        encrypted[last] ^= 0xFF;
        // Ciphertext corruption invalidates the HMAC (it covers the ciphertext).
        assert!(decrypt(&encrypted, "pass").is_err());
    }

    #[test]
    fn salt_iv_randomness() {
        let a = encrypt("same text", "same password").unwrap();
        let b = encrypt("same text", "same password").unwrap();
        assert_ne!(&a[..SALT_SIZE], &b[..SALT_SIZE], "salts must differ");
        assert_ne!(
            &a[SALT_SIZE..SALT_SIZE + IV_SIZE],
            &b[SALT_SIZE..SALT_SIZE + IV_SIZE],
            "IVs must differ"
        );
        assert_ne!(a, b);
    }

    #[test]
    fn wire_format_layout() {
        let encrypted = encrypt("test", "pass").unwrap();
        assert!(encrypted.len() >= MIN_PAYLOAD_SIZE);
        let ct_len = encrypted.len() - SALT_SIZE - IV_SIZE - ARGON2_PARAMS_SIZE - HMAC_SIZE;
        assert_eq!(ct_len % 16, 0);
    }

    #[test]
    fn minimum_payload_size_is_correct() {
        assert_eq!(MIN_PAYLOAD_SIZE, 92);
    }

    #[test]
    fn derive_keys_deterministic() {
        let salt = [0x42u8; SALT_SIZE];
        let (k1_enc, k1_mac) =
            derive_keys("password", &salt, DEFAULT_M_COST, DEFAULT_T_COST, DEFAULT_P_LANES).unwrap();
        let (k2_enc, k2_mac) =
            derive_keys("password", &salt, DEFAULT_M_COST, DEFAULT_T_COST, DEFAULT_P_LANES).unwrap();
        assert_eq!(k1_enc, k2_enc);
        assert_eq!(k1_mac, k2_mac);
    }

    #[test]
    fn derive_keys_different_passwords() {
        let salt = [0x42u8; SALT_SIZE];
        let (k1_enc, _) =
            derive_keys("password1", &salt, DEFAULT_M_COST, DEFAULT_T_COST, DEFAULT_P_LANES).unwrap();
        let (k2_enc, _) =
            derive_keys("password2", &salt, DEFAULT_M_COST, DEFAULT_T_COST, DEFAULT_P_LANES).unwrap();
        assert_ne!(k1_enc, k2_enc);
    }

    #[test]
    fn derive_keys_different_salts() {
        let (k1_enc, _) = derive_keys(
            "password",
            &[0x01u8; SALT_SIZE],
            DEFAULT_M_COST,
            DEFAULT_T_COST,
            DEFAULT_P_LANES,
        )
        .unwrap();
        let (k2_enc, _) = derive_keys(
            "password",
            &[0x02u8; SALT_SIZE],
            DEFAULT_M_COST,
            DEFAULT_T_COST,
            DEFAULT_P_LANES,
        )
        .unwrap();
        assert_ne!(k1_enc, k2_enc);
    }

    #[test]
    fn plaintext_on_block_boundary() {
        let plaintext = "0123456789abcdef"; // 16 bytes
        let encrypted = encrypt(plaintext, "pass").unwrap();
        let ct_len = encrypted.len() - SALT_SIZE - IV_SIZE - ARGON2_PARAMS_SIZE - HMAC_SIZE;
        assert_eq!(ct_len, 32); // 16 data + 16 padding
        assert_eq!(decrypt(&encrypted, "pass").unwrap(), plaintext);
    }

    #[test]
    fn default_constants() {
        assert_eq!(DEFAULT_M_COST, 65_536);
        assert_eq!(DEFAULT_T_COST, 3);
        assert_eq!(DEFAULT_P_LANES, 4);
    }

    #[test]
    fn ciphertext_length_always_multiple_of_16() {
        for len in [0, 1, 15, 16, 17, 31, 32, 33, 100] {
            let plaintext: String = "X".repeat(len);
            let encrypted = encrypt(&plaintext, "pass").unwrap();
            let ct_len = encrypted.len() - SALT_SIZE - IV_SIZE - ARGON2_PARAMS_SIZE - HMAC_SIZE;
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
        let a = encrypt("same text", "same pass").unwrap();
        let b = encrypt("same text", "same pass").unwrap();
        assert_ne!(a, b, "two encryptions of identical input must differ");
    }

    #[test]
    fn empty_password() {
        let plaintext = "secret data";
        let encrypted = encrypt(plaintext, "").unwrap();
        assert_eq!(decrypt(&encrypted, "").unwrap(), plaintext);
    }

    #[test]
    fn very_long_password() {
        let password: String = "A".repeat(10_000);
        let plaintext = "protected by a very long password";
        let encrypted = encrypt(plaintext, &password).unwrap();
        assert_eq!(decrypt(&encrypted, &password).unwrap(), plaintext);
    }

    #[test]
    fn password_with_unicode() {
        let password = "p\u{00E4}ssw\u{00F6}rd\u{1F512}\u{1F525}\u{2603}";
        let plaintext = "emoji-locked content";
        let encrypted = encrypt(plaintext, password).unwrap();
        assert_eq!(decrypt(&encrypted, password).unwrap(), plaintext);
    }

    #[test]
    fn corrupted_salt_returns_err() {
        let mut encrypted = encrypt("test data", "password").unwrap();
        encrypted[7] ^= 0xFF;
        assert!(decrypt(&encrypted, "password").is_err());
    }

    #[test]
    fn corrupted_iv_returns_err() {
        let mut encrypted = encrypt("test data", "password").unwrap();
        encrypted[20] ^= 0xFF;
        assert!(decrypt(&encrypted, "password").is_err());
    }
}
