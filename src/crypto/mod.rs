// Crypto module — AES-256-CBC + HMAC-SHA256, Argon2id
//
// Wire format:
//   [salt 16][iv 16][m_cost 4 LE][t_cost 4 LE][p_lanes 4 LE][hmac 32][ciphertext ...]
//   HMAC is computed over salt || iv || m_cost || t_cost || p_lanes || ciphertext.
//
// All sensitive buffers are zeroed after use via `zeroize`.

use aes::Aes256;
use argon2::Argon2;
use cbc::cipher::{BlockModeDecrypt, BlockModeEncrypt, KeyIvInit};
use hmac::{Hmac, KeyInit, Mac};
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

/// RAII-locked page of memory for short-lived key material.
///
/// On Windows the buffer is allocated via `VirtualAlloc` and pinned in
/// physical memory with `VirtualLock` so the page cannot be written to
/// the swap file under memory pressure (or to hiberfil.sys at sleep).
/// On drop the page is explicitly zeroed, unlocked, and freed.
///
/// On other platforms this falls back to a plain zeroizing Vec — good
/// for tests, and we don't ship on non-Windows anyway.
#[cfg(windows)]
struct SecureKeyBuffer {
    ptr: *mut u8,
    page_size: usize,
    len: usize,
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn VirtualAlloc(
        lpAddress: *mut core::ffi::c_void,
        dwSize: usize,
        flAllocationType: u32,
        flProtect: u32,
    ) -> *mut core::ffi::c_void;
    fn VirtualLock(lpAddress: *mut core::ffi::c_void, dwSize: usize) -> i32;
    fn VirtualUnlock(lpAddress: *mut core::ffi::c_void, dwSize: usize) -> i32;
    fn VirtualFree(lpAddress: *mut core::ffi::c_void, dwSize: usize, dwFreeType: u32) -> i32;
}

#[cfg(windows)]
const MEM_COMMIT: u32 = 0x0000_1000;
#[cfg(windows)]
const MEM_RESERVE: u32 = 0x0000_2000;
#[cfg(windows)]
const MEM_RELEASE: u32 = 0x0000_8000;
#[cfg(windows)]
const PAGE_READWRITE: u32 = 0x04;

#[cfg(windows)]
impl SecureKeyBuffer {
    fn new(len: usize) -> Result<Self, CryptoError> {
        // One page is plenty for our 64-byte key_material; overallocating
        // avoids any alignment dance at the cost of a handful of KiB.
        let page_size: usize = 4096;
        assert!(len <= page_size);
        let ptr = unsafe {
            VirtualAlloc(std::ptr::null_mut(), page_size, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE)
        } as *mut u8;
        if ptr.is_null() {
            return Err(CryptoError::InvalidParameters(
                "VirtualAlloc for secure key buffer failed".to_string(),
            ));
        }
        unsafe {
            // VirtualLock can fail silently if the process working-set
            // quota is tight — we still proceed, but the bytes are at
            // worst as safe as a regular stack allocation was before.
            let _ = VirtualLock(ptr as *mut _, page_size);
        }
        Ok(SecureKeyBuffer { ptr, page_size, len })
    }

    fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}

#[cfg(windows)]
impl Drop for SecureKeyBuffer {
    fn drop(&mut self) {
        unsafe {
            // Explicit overwrite before freeing. Uses volatile writes so
            // the optimiser cannot elide the loop (which `[u8]::zeroize`
            // also guarantees, but doing it here keeps the invariant
            // local to this module and independent of the Zeroize impl).
            for i in 0..self.page_size {
                std::ptr::write_volatile(self.ptr.add(i), 0);
            }
            let _ = VirtualUnlock(self.ptr as *mut _, self.page_size);
            let _ = VirtualFree(self.ptr as *mut _, 0, MEM_RELEASE);
        }
    }
}

/// Derives a 32-byte encryption key and a 32-byte MAC key from a password
/// and salt using Argon2id with the specified parameters.
fn derive_keys(
    password: &str,
    salt: &[u8],
    m_cost: u32,
    t_cost: u32,
    p_lanes: u32,
) -> Result<([u8; KEY_SIZE], [u8; KEY_SIZE]), CryptoError> {
    // Allocate the 64-byte Argon2 output in a locked page so the derived
    // keys cannot be paged to disk before we copy them into the short-
    // lived stack arrays returned to the caller.
    #[cfg(windows)]
    let mut km_buf = SecureKeyBuffer::new(64)?;
    #[cfg(windows)]
    let key_material = km_buf.as_mut_slice();
    #[cfg(not(windows))]
    let mut km_stack = [0u8; 64];
    #[cfg(not(windows))]
    let key_material: &mut [u8] = &mut km_stack;

    let params = argon2::Params::new(m_cost, t_cost, p_lanes, Some(64))
        .map_err(|e| CryptoError::InvalidParameters(e.to_string()))?;
    let argon2 = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    argon2
        .hash_password_into(password.as_bytes(), salt, key_material)
        .map_err(|e| CryptoError::KdfFailed(e.to_string()))?;

    let mut enc_key = [0u8; KEY_SIZE];
    let mut mac_key = [0u8; KEY_SIZE];
    enc_key.copy_from_slice(&key_material[..KEY_SIZE]);
    mac_key.copy_from_slice(&key_material[KEY_SIZE..]);

    // key_material itself is wiped by SecureKeyBuffer::drop (Windows) or
    // by the explicit zeroize on the stack buffer (non-Windows).
    #[cfg(not(windows))]
    km_stack.zeroize();
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

/// Encrypts `plaintext` with `password` using the specified Argon2id parameters
/// and returns the wire-format payload.
///
/// Wire format: `salt[16] || iv[16] || m_cost[4 LE] || t_cost[4 LE] || p_lanes[4 LE] || hmac[32] || ciphertext[...]`
pub fn encrypt(
    plaintext: &str,
    password: &str,
    m_cost: u32,
    t_cost: u32,
    p_lanes: u32,
) -> Result<Vec<u8>, CryptoError> {
    // 1. Generate random salt and IV
    let mut salt = [0u8; SALT_SIZE];
    let mut iv = [0u8; IV_SIZE];
    let mut rng = rand::rng();
    rng.fill_bytes(&mut salt);
    rng.fill_bytes(&mut iv);
    let output = encrypt_with(plaintext, password, m_cost, t_cost, p_lanes, &salt, &iv);
    salt.zeroize();
    iv.zeroize();
    output
}

/// `encrypt` with a given salt and IV (the known-answer tests pin its bytes).
fn encrypt_with(
    plaintext: &str,
    password: &str,
    m_cost: u32,
    t_cost: u32,
    p_lanes: u32,
    salt: &[u8; SALT_SIZE],
    iv: &[u8; IV_SIZE],
) -> Result<Vec<u8>, CryptoError> {
    // Local copies, wiped at the end like the rest.
    let mut salt = *salt;
    let mut iv = *iv;

    // 2. Derive keys
    let (mut enc_key, mut mac_key) = derive_keys(password, &salt, m_cost, t_cost, p_lanes)?;

    // 3. Encrypt (AES-256-CBC, PKCS7 padding)
    let mut plain_bytes = plaintext.as_bytes().to_vec();
    let padded_len = (plain_bytes.len() / 16 + 1) * 16;
    let mut buf = vec![0u8; padded_len];
    buf[..plain_bytes.len()].copy_from_slice(&plain_bytes);

    let ciphertext = Aes256CbcEnc::new(&enc_key.into(), &iv.into())
        .encrypt_padded::<cbc::cipher::block_padding::Pkcs7>(&mut buf, plain_bytes.len())
        .map_err(|e| CryptoError::Encryption(e.to_string()))?
        .to_vec();

    // 4. Encode Argon2id parameters as 12 bytes little-endian
    let mut params_bytes = [0u8; ARGON2_PARAMS_SIZE];
    params_bytes[0..4].copy_from_slice(&m_cost.to_le_bytes());
    params_bytes[4..8].copy_from_slice(&t_cost.to_le_bytes());
    params_bytes[8..12].copy_from_slice(&p_lanes.to_le_bytes());

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
    // `iv` is IV_SIZE bytes by the slicing above.
    let iv: [u8; IV_SIZE] = match iv.try_into() {
        Ok(a) => a,
        Err(_) => {
            enc_key.zeroize();
            mac_key.zeroize();
            return Err(CryptoError::InvalidInput("IV length".into()));
        }
    };
    let plaintext_result = Aes256CbcDec::new(&enc_key.into(), &iv.into())
        .decrypt_padded::<cbc::cipher::block_padding::Pkcs7>(&mut ct_buf);

    enc_key.zeroize();
    mac_key.zeroize();

    match plaintext_result {
        Ok(plain_bytes) => {
            let owned = plain_bytes.to_vec();
            ct_buf.zeroize();
            // Post-HMAC: a UTF-8 failure means the HMAC-authenticated
            // plaintext somehow isn't valid UTF-8. That should not happen
            // in normal operation and, if it does, the caller gets no
            // benefit from distinguishing this from a bad password. Flatten
            // to HmacMismatch so UI code cannot build a padding/utf8 oracle
            // out of the variants.
            String::from_utf8(owned).map_err(|_| CryptoError::HmacMismatch)
        }
        Err(_) => {
            // Post-HMAC PKCS7 padding failure. Unreachable without key
            // knowledge (the MAC already passed), so collapse into
            // HmacMismatch for the same reason as above.
            ct_buf.zeroize();
            Err(CryptoError::HmacMismatch)
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Use fast Argon2id params for tests to avoid slow CI
    const TEST_M: u32 = 256; // 256 KiB
    const TEST_T: u32 = 1;
    const TEST_P: u32 = 1;

    #[test]
    fn round_trip_basic() {
        let password = "correct horse battery staple";
        let plaintext = "Hello, LockNote!";
        let encrypted = encrypt(plaintext, password, TEST_M, TEST_T, TEST_P).unwrap();
        let decrypted = decrypt(&encrypted, password).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn round_trip_empty_plaintext() {
        let password = "pass";
        let plaintext = "";
        let encrypted = encrypt(plaintext, password, TEST_M, TEST_T, TEST_P).unwrap();
        assert!(encrypted.len() >= MIN_PAYLOAD_SIZE);
        let decrypted = decrypt(&encrypted, password).unwrap();
        assert_eq!(decrypted, String::new());
    }

    #[test]
    fn round_trip_unicode() {
        let password = "mdp";
        let plaintext = "Caf\u{00e9} \u{1f512}\u{1f4dd} \u{65e5}\u{672c}\u{8a9e} \u{0410}\u{0411}\u{0412}";
        let encrypted = encrypt(plaintext, password, TEST_M, TEST_T, TEST_P).unwrap();
        let decrypted = decrypt(&encrypted, password).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn round_trip_large_text() {
        let password = "big";
        let plaintext: String = "A".repeat(100_000);
        let encrypted = encrypt(&plaintext, password, TEST_M, TEST_T, TEST_P).unwrap();
        let decrypted = decrypt(&encrypted, password).unwrap();
        assert_eq!(decrypted.len(), 100_000);
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn wrong_password_returns_hmac_mismatch() {
        let encrypted = encrypt("secret", "right", TEST_M, TEST_T, TEST_P).unwrap();
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
        let mut encrypted = encrypt("data", "pass", TEST_M, TEST_T, TEST_P).unwrap();
        // Flip a byte in the HMAC region (offset 44..76)
        encrypted[50] ^= 0xFF;
        assert!(matches!(decrypt(&encrypted, "pass"), Err(CryptoError::HmacMismatch)));
    }

    #[test]
    fn corrupted_ciphertext_returns_err() {
        let mut encrypted = encrypt("data", "pass", TEST_M, TEST_T, TEST_P).unwrap();
        let last = encrypted.len() - 1;
        encrypted[last] ^= 0xFF;
        // Ciphertext corruption invalidates the HMAC (it covers the ciphertext).
        assert!(decrypt(&encrypted, "pass").is_err());
    }

    #[test]
    fn salt_iv_randomness() {
        let a = encrypt("same text", "same password", TEST_M, TEST_T, TEST_P).unwrap();
        let b = encrypt("same text", "same password", TEST_M, TEST_T, TEST_P).unwrap();
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
        let encrypted = encrypt("test", "pass", TEST_M, TEST_T, TEST_P).unwrap();
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
        let (k1_enc, k1_mac) = derive_keys("password", &salt, TEST_M, TEST_T, TEST_P).unwrap();
        let (k2_enc, k2_mac) = derive_keys("password", &salt, TEST_M, TEST_T, TEST_P).unwrap();
        assert_eq!(k1_enc, k2_enc);
        assert_eq!(k1_mac, k2_mac);
    }

    #[test]
    fn derive_keys_different_passwords() {
        let salt = [0x42u8; SALT_SIZE];
        let (k1_enc, _) = derive_keys("password1", &salt, TEST_M, TEST_T, TEST_P).unwrap();
        let (k2_enc, _) = derive_keys("password2", &salt, TEST_M, TEST_T, TEST_P).unwrap();
        assert_ne!(k1_enc, k2_enc);
    }

    #[test]
    fn derive_keys_different_salts() {
        let (k1_enc, _) = derive_keys("password", &[0x01u8; SALT_SIZE], TEST_M, TEST_T, TEST_P).unwrap();
        let (k2_enc, _) = derive_keys("password", &[0x02u8; SALT_SIZE], TEST_M, TEST_T, TEST_P).unwrap();
        assert_ne!(k1_enc, k2_enc);
    }

    #[test]
    fn plaintext_on_block_boundary() {
        let plaintext = "0123456789abcdef"; // 16 bytes
        let encrypted = encrypt(plaintext, "pass", TEST_M, TEST_T, TEST_P).unwrap();
        let ct_len = encrypted.len() - SALT_SIZE - IV_SIZE - ARGON2_PARAMS_SIZE - HMAC_SIZE;
        assert_eq!(ct_len, 32); // 16 data + 16 padding
        assert_eq!(decrypt(&encrypted, "pass").unwrap(), plaintext);
    }

    #[test]
    fn argon2_params_stored_in_payload() {
        let encrypted = encrypt("test", "pass", 1024, 2, 3).unwrap();
        let params_bytes = &encrypted[SALT_SIZE + IV_SIZE..SALT_SIZE + IV_SIZE + ARGON2_PARAMS_SIZE];
        let m = u32::from_le_bytes([params_bytes[0], params_bytes[1], params_bytes[2], params_bytes[3]]);
        let t = u32::from_le_bytes([params_bytes[4], params_bytes[5], params_bytes[6], params_bytes[7]]);
        let p = u32::from_le_bytes([params_bytes[8], params_bytes[9], params_bytes[10], params_bytes[11]]);
        assert_eq!(m, 1024);
        assert_eq!(t, 2);
        assert_eq!(p, 3);
    }

    #[test]
    fn different_params_produce_different_keys() {
        let salt = [0x42u8; SALT_SIZE];
        let (k1, _) = derive_keys("password", &salt, 256, 1, 1).unwrap();
        let (k2, _) = derive_keys("password", &salt, 512, 1, 1).unwrap();
        assert_ne!(k1, k2);
    }

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{:02x}", x)).collect()
    }

    /// Known answers (computed before the dependency update of 2026-11-26
    /// with aes 0.8, cbc 0.1, hmac 0.12, sha2 0.10, argon2 0.5): the bytes
    /// written and the keys derived must never change, or existing notes
    /// would no longer open.
    #[test]
    fn argon2id_keys_are_pinned() {
        let (enc, mac) = derive_keys("correct horse", &[0x5au8; SALT_SIZE], 256, 2, 2).unwrap();
        assert_eq!(hex(&enc), "78b0290fa0f20fc71e5a499458dcb69b7f158e28627d934755e6507afeabe61d");
        assert_eq!(hex(&mac), "3afe3c32fab3f2b7ead2b28b986a1bc5b24ca22aaa2b0b2b96c88156df0b6a98");
    }

    #[test]
    fn encrypted_payload_bytes_are_pinned() {
        let salt: [u8; SALT_SIZE] = core::array::from_fn(|i| i as u8);
        let iv: [u8; IV_SIZE] = core::array::from_fn(|i| 0xf0 ^ i as u8);
        let plain = "LockNote KAT \u{2014} \u{e9}t\u{e9} 0123456789abcdef!";
        let out = encrypt_with(plain, "correct horse", 256, 2, 2, &salt, &iv).unwrap();
        assert_eq!(hex(&out), "000102030405060708090a0b0c0d0e0ff0f1f2f3f4f5f6f7f8f9fafbfcfdfeff00010000020000000200000093ac0336f440718903bc82b4213be40f629e903f15215437a08a152fdc46724617a8ddbb5321704f076c31165366adc4f4627ea1914ffcd52d8688bd56eeceefbf04b149ec21d5f32008b6b08a00e237");
        assert_eq!(decrypt(&out, "correct horse").unwrap(), plain);
    }

    #[test]
    fn default_constants() {
        assert_eq!(DEFAULT_M_COST, 65_536);
        assert_eq!(DEFAULT_T_COST, 3);
        assert_eq!(DEFAULT_P_LANES, 4);
    }
}
