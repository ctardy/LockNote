// Crypto module — AES-256-CBC + HMAC-SHA256, Argon2id
//
// Wire format:
//   [salt 16][iv 16][m_cost 4 LE][t_cost 4 LE][p_lanes 4 LE][hmac 32][ciphertext ...]
//   HMAC is computed over salt || iv || m_cost || t_cost || p_lanes || ciphertext.
//
// All sensitive buffers are zeroed after use via `zeroize`, on every way
// out (issue #42): the keys, the Argon2 working memory, the padded plaintext
// buffer of `encrypt` and the decryption buffer of `decrypt`, which becomes
// the returned text (a `Zeroizing<String>`, never copied). The AES key
// schedule and the CBC state wipe themselves on drop (their "zeroize"
// features), and so do the Argon2 intermediate hashes. Out of reach: the
// keyed HMAC state (hmac 0.13 does not wipe it; MAC key material only), and
// stack copies made by moves of the key arrays and inside the crates.

use aes::Aes256;
use argon2::Argon2;
use cbc::cipher::{BlockModeDecrypt, BlockModeEncrypt, KeyIvInit};
use hmac::{Hmac, KeyInit, Mac};
use rand::Rng;
use sha2::Sha256;
use zeroize::{Zeroize, Zeroizing};

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
    // The Argon2 working memory (m_cost KiB) is derived from the password and
    // its last blocks make the keys: given by us so it is wiped before it is
    // freed (`hash_password_into` frees its own memory unwiped).
    let mut blocks: Zeroizing<Vec<argon2::Block>> = Zeroizing::new(vec![argon2::Block::new(); params.block_count()]);
    let argon2 = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    argon2
        .hash_password_into_with_memory(password.as_bytes(), salt, key_material, &mut blocks[..])
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

    // 2. Derive keys. Nothing between here and the wipe returns early: the
    // fallible steps are in `seal`.
    let (mut enc_key, mut mac_key) = derive_keys(password, &salt, m_cost, t_cost, p_lanes)?;
    let sealed = seal(plaintext, &enc_key, &mac_key, &salt, &iv, m_cost, t_cost, p_lanes);

    // 7. Zeroize sensitive material
    enc_key.zeroize();
    mac_key.zeroize();
    salt.zeroize();
    iv.zeroize();
    sealed
}

/// Steps 3 to 6 of `encrypt_with`, with the keys derived.
#[allow(clippy::too_many_arguments)]
fn seal(
    plaintext: &str,
    enc_key: &[u8; KEY_SIZE],
    mac_key: &[u8; KEY_SIZE],
    salt: &[u8; SALT_SIZE],
    iv: &[u8; IV_SIZE],
    m_cost: u32,
    t_cost: u32,
    p_lanes: u32,
) -> Result<Vec<u8>, CryptoError> {
    // 3. Encrypt in place (AES-256-CBC, PKCS7 padding): the plaintext is
    // copied once, into the buffer sized for its padding, wiped on every way
    // out.
    let plain = plaintext.as_bytes();
    let mut buf: Zeroizing<Vec<u8>> = Zeroizing::new(vec![0u8; (plain.len() / 16 + 1) * 16]);
    buf[..plain.len()].copy_from_slice(plain);

    let ciphertext = Aes256CbcEnc::new(&(*enc_key).into(), &(*iv).into())
        .encrypt_padded::<cbc::cipher::block_padding::Pkcs7>(&mut buf, plain.len())
        .map_err(|e| CryptoError::Encryption(e.to_string()))?;

    // 4. Encode Argon2id parameters as 12 bytes little-endian
    let mut params_bytes = [0u8; ARGON2_PARAMS_SIZE];
    params_bytes[0..4].copy_from_slice(&m_cost.to_le_bytes());
    params_bytes[4..8].copy_from_slice(&t_cost.to_le_bytes());
    params_bytes[8..12].copy_from_slice(&p_lanes.to_le_bytes());

    // 5. Compute HMAC over salt || iv || params || ciphertext
    let hmac_value = compute_hmac(mac_key, salt, iv, &params_bytes, ciphertext)?;

    // 6. Assemble output: salt || iv || params || hmac || ciphertext
    let mut output =
        Vec::with_capacity(SALT_SIZE + IV_SIZE + ARGON2_PARAMS_SIZE + HMAC_SIZE + ciphertext.len());
    output.extend_from_slice(salt);
    output.extend_from_slice(iv);
    output.extend_from_slice(&params_bytes);
    output.extend_from_slice(&hmac_value);
    output.extend_from_slice(ciphertext);
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
///
/// The text is decrypted in place, in a copy of the ciphertext, and that
/// buffer is returned as the text: wiped when the caller drops it.
pub fn decrypt(data: &[u8], password: &str) -> Result<Zeroizing<String>, CryptoError> {
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

    // Decrypt in place: `buf` ends holding the plaintext, then becomes the
    // returned text (no copy). Wiped on every way out.
    let mut buf: Zeroizing<Vec<u8>> = Zeroizing::new(ciphertext.to_vec());
    // `iv` is IV_SIZE bytes by the slicing above.
    let iv: [u8; IV_SIZE] = match iv.try_into() {
        Ok(a) => a,
        Err(_) => {
            enc_key.zeroize();
            mac_key.zeroize();
            return Err(CryptoError::InvalidInput("IV length".into()));
        }
    };
    let plaintext_len = Aes256CbcDec::new(&enc_key.into(), &iv.into())
        .decrypt_padded::<cbc::cipher::block_padding::Pkcs7>(&mut buf)
        .map(|plain| plain.len());

    enc_key.zeroize();
    mac_key.zeroize();

    // Post-HMAC PKCS7 padding failure. Unreachable without key knowledge
    // (the MAC already passed), so collapse into HmacMismatch for the same
    // reason as the UTF-8 failure below.
    let Ok(plaintext_len) = plaintext_len else {
        return Err(CryptoError::HmacMismatch);
    };
    // The padding bytes stay in the spare capacity, wiped with the rest.
    buf.truncate(plaintext_len);
    match String::from_utf8(std::mem::take(&mut *buf)) {
        Ok(text) => Ok(Zeroizing::new(text)),
        Err(e) => {
            // Post-HMAC: a UTF-8 failure means the HMAC-authenticated
            // plaintext somehow isn't valid UTF-8. That should not happen
            // in normal operation and, if it does, the caller gets no
            // benefit from distinguishing this from a bad password. Flatten
            // to HmacMismatch so UI code cannot build a padding/utf8 oracle
            // out of the variants.
            e.into_bytes().zeroize();
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
        assert_eq!(decrypted.as_str(), plaintext);
    }

    #[test]
    fn round_trip_empty_plaintext() {
        let password = "pass";
        let plaintext = "";
        let encrypted = encrypt(plaintext, password, TEST_M, TEST_T, TEST_P).unwrap();
        assert!(encrypted.len() >= MIN_PAYLOAD_SIZE);
        let decrypted = decrypt(&encrypted, password).unwrap();
        assert_eq!(decrypted.as_str(), "");
    }

    #[test]
    fn round_trip_unicode() {
        let password = "mdp";
        let plaintext = "Caf\u{00e9} \u{1f512}\u{1f4dd} \u{65e5}\u{672c}\u{8a9e} \u{0410}\u{0411}\u{0412}";
        let encrypted = encrypt(plaintext, password, TEST_M, TEST_T, TEST_P).unwrap();
        let decrypted = decrypt(&encrypted, password).unwrap();
        assert_eq!(decrypted.as_str(), plaintext);
    }

    #[test]
    fn round_trip_large_text() {
        let password = "big";
        let plaintext: String = "A".repeat(100_000);
        let encrypted = encrypt(&plaintext, password, TEST_M, TEST_T, TEST_P).unwrap();
        let decrypted = decrypt(&encrypted, password).unwrap();
        assert_eq!(decrypted.len(), 100_000);
        assert_eq!(decrypted.as_str(), plaintext);
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
        assert_eq!(decrypt(&encrypted, "pass").unwrap().as_str(), plaintext);
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
        assert_eq!(decrypt(&out, "correct horse").unwrap().as_str(), plain);
    }

    #[test]
    fn default_constants() {
        assert_eq!(DEFAULT_M_COST, 65_536);
        assert_eq!(DEFAULT_T_COST, 3);
        assert_eq!(DEFAULT_P_LANES, 4);
    }

    // ---- Issue #42 characterisation (written and green before the change) ----

    /// The wire bytes at the padding edges: empty plaintext (a whole block of
    /// padding), one byte short of a block, exactly a block, and a
    /// multi-block non-ASCII text.
    #[test]
    fn issue42_encrypted_bytes_at_block_bounds_are_pinned() {
        let salt: [u8; SALT_SIZE] = core::array::from_fn(|i| 0x10 + i as u8);
        let iv: [u8; IV_SIZE] = core::array::from_fn(|i| 0xa0 ^ i as u8);
        let mut got = Vec::new();
        for plain in ["", "0123456789abcde", "0123456789abcdef", "\u{e9}t\u{e9}\r\n\u{1f512} multi block \u{6f22}\u{5b57} text"] {
            let out = encrypt_with(plain, "pw", 64, 1, 1, &salt, &iv).unwrap();
            assert_eq!(decrypt(&out, "pw").unwrap().as_str(), plain);
            got.push(hex(&out[SALT_SIZE + IV_SIZE + ARGON2_PARAMS_SIZE..]));
        }
        assert_eq!(format!("{:?}", got), "[\"754efcfb4785b93ece6d860ac60117c22f87ad1e9427b580fc288d51da5ea3d2fc7ec31a20466d8863fe72b3289b68df\", \"ca1341f65d863c627480884c472ebad5d494dfbfac8cdfc173e6fea049f01e33f0c65dbdad31779fe983a4d7376bb056\", \"fb3b38d048871cb7ebdb5c2e7fc00f44526ce24f5616401bcb0dbe26f9e5ad91c83f8ddfa7d89ca65b04bed65775bbd4fccf4e7ebd9b2d0899c85084e39cb33b\", \"3ed4e1cd971a6ab6bc5f8e8f9a2e2d74b720ee63bc6d51b88424269e02b61bb17404c2150c4148fa778ef2fe6ad5332077b28d840aa840c3c01572b6111a22402ade2acbbb5fad242f8077ef5e03066e\"]");
    }

    #[test]
    fn issue42_round_trip_every_length_up_to_three_blocks() {
        for n in 0..=49 {
            let plain: String = "a\u{e9}\u{6f22}".chars().cycle().take(n).collect();
            let out = encrypt(&plain, "pw", TEST_M, TEST_T, TEST_P).unwrap();
            let ct_len = out.len() - SALT_SIZE - IV_SIZE - ARGON2_PARAMS_SIZE - HMAC_SIZE;
            assert_eq!(ct_len, (plain.len() / 16 + 1) * 16, "{n}");
            assert_eq!(decrypt(&out, "pw").unwrap().as_str(), plain, "{n}");
        }
    }

    /// AES-CBC + HMAC over raw bytes, laid out like `encrypt_with`: lets a
    /// test authenticate a plaintext `encrypt` cannot produce (not UTF-8).
    fn issue42_encrypt_raw(plain: &[u8], password: &str) -> Vec<u8> {
        let salt = [7u8; SALT_SIZE];
        let iv = [9u8; IV_SIZE];
        let (enc_key, mac_key) = derive_keys(password, &salt, 64, 1, 1).unwrap();
        let mut buf = vec![0u8; (plain.len() / 16 + 1) * 16];
        buf[..plain.len()].copy_from_slice(plain);
        let ct = Aes256CbcEnc::new(&enc_key.into(), &iv.into())
            .encrypt_padded::<cbc::cipher::block_padding::Pkcs7>(&mut buf, plain.len())
            .unwrap()
            .to_vec();
        let mut params = [0u8; ARGON2_PARAMS_SIZE];
        params[0..4].copy_from_slice(&64u32.to_le_bytes());
        params[4..8].copy_from_slice(&1u32.to_le_bytes());
        params[8..12].copy_from_slice(&1u32.to_le_bytes());
        let mac = compute_hmac(&mac_key, &salt, &iv, &params, &ct).unwrap();
        [&salt[..], &iv[..], &params[..], &mac[..], &ct[..]].concat()
    }

    #[test]
    fn issue42_authenticated_non_utf8_plaintext_reads_as_hmac_mismatch() {
        let ok = issue42_encrypt_raw("fine \u{e9}".as_bytes(), "pw");
        assert_eq!(decrypt(&ok, "pw").unwrap().as_str(), "fine \u{e9}");
        for bad in [&[0xffu8][..], &[0x61, 0xc3][..], &[0xed, 0xa0, 0x80][..], &[0x61; 40][..39].iter().copied().chain([0x80]).collect::<Vec<u8>>()[..]] {
            let wire = issue42_encrypt_raw(bad, "pw");
            assert!(matches!(decrypt(&wire, "pw"), Err(CryptoError::HmacMismatch)), "{:?}", bad);
        }
    }

    // ---- Issue #42 ----

    /// The decrypted text is wrapped (wiped on drop) from `decrypt` up to
    /// the unlock dialog's callers, and the AES key schedule and the CBC
    /// states wipe themselves (their "zeroize" features): checked by the
    /// compiler. (hmac 0.13 has no wiping of its keyed state.)
    #[test]
    fn issue42_decrypted_text_is_wrapped_up_to_the_callers() {
        fn wipes_on_drop<T: zeroize::ZeroizeOnDrop>() {}
        let _: fn(&[u8], &str) -> Result<Zeroizing<String>, CryptoError> = decrypt;
        let _: fn(&[u8]) -> Option<(Zeroizing<String>, Zeroizing<String>)> = crate::ui::dialogs::unlock::show;
        let _: fn(&[u8], isize) -> Option<(Zeroizing<String>, Zeroizing<String>)> = crate::ui::dialogs::unlock::show_with_parent;
        wipes_on_drop::<Aes256>();
        wipes_on_drop::<Aes256CbcEnc>();
        wipes_on_drop::<Aes256CbcDec>();
    }

    /// `decrypt` hands back the buffer it decrypted in (its capacity is the
    /// ciphertext's length), not a copy of the plaintext.
    #[test]
    fn issue42_decrypt_returns_its_decryption_buffer() {
        for plain in ["", "a", "0123456789abcdef", "\u{e9}t\u{e9} \u{1f512} and some more text"] {
            let out = encrypt(plain, "pw", TEST_M, TEST_T, TEST_P).unwrap();
            let ct_len = out.len() - SALT_SIZE - IV_SIZE - ARGON2_PARAMS_SIZE - HMAC_SIZE;
            let text = decrypt(&out, "pw").unwrap();
            assert_eq!((text.as_str(), text.capacity()), (plain, ct_len));
        }
    }

    fn issue42_fn_body(src: &str, start: &str) -> String {
        let at = src.find(start).unwrap_or_else(|| panic!("{start}"));
        let len = src[at..].find("\n}\n").unwrap();
        src[at..at + len].to_string()
    }

    /// The wiping that tests cannot watch (freed memory), pinned in the source.
    #[test]
    fn issue42_crypto_buffers_are_wiped_on_every_way_out() {
        let src = crate::read_source("src/crypto/mod.rs");
        let derive = issue42_fn_body(&src, "fn derive_keys(");
        assert!(derive.contains("let mut blocks: Zeroizing<Vec<argon2::Block>> = Zeroizing::new(vec![argon2::Block::new(); params.block_count()]);"));
        assert!(derive.contains(".hash_password_into_with_memory(password.as_bytes(), salt, key_material, &mut blocks[..])"));
        assert!(!derive.contains(".hash_password_into("), "Argon2 memory allocated (and freed unwiped) by the crate");

        let enc = issue42_fn_body(&src, "fn encrypt_with(");
        let from_keys = &enc[enc.find("let (mut enc_key, mut mac_key) = derive_keys(").unwrap()..];
        let until_wipe = &from_keys[..from_keys.find("enc_key.zeroize();").unwrap()];
        assert!(until_wipe.contains("let sealed = seal(plaintext, &enc_key, &mac_key, &salt, &iv, m_cost, t_cost, p_lanes);"));
        assert_eq!(until_wipe.matches('?').count(), 1, "an early return skips the key wipe: {until_wipe}");
        assert!(enc.contains("mac_key.zeroize();"));

        let seal = issue42_fn_body(&src, "fn seal(");
        assert!(seal.contains("let mut buf: Zeroizing<Vec<u8>> = Zeroizing::new(vec![0u8; (plain.len() / 16 + 1) * 16]);"));
        assert!(!seal.contains("to_vec()"), "a copy of the plaintext or the ciphertext");

        let dec = issue42_fn_body(&src, "pub fn decrypt(");
        assert!(dec.contains("let mut buf: Zeroizing<Vec<u8>> = Zeroizing::new(ciphertext.to_vec());"));
        assert!(dec.contains("match String::from_utf8(std::mem::take(&mut *buf)) {"));
        assert!(dec.contains("e.into_bytes().zeroize();"), "a non-UTF-8 plaintext freed unwiped");
        assert_eq!(dec.matches("to_vec()").count(), 1, "one copy: the ciphertext, decrypted in place");
    }

    /// Every caller of `decrypt` keeps its wrapper or moves the text out of
    /// it: no copy on the way.
    #[test]
    fn issue42_decrypted_text_is_moved_not_copied() {
        let read = |p: &str| crate::read_source(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(p));
        let unlock = read("src/ui/dialogs/unlock.rs");
        assert!(unlock.contains(
            "                            Ok(plaintext) => {\n                                *result_clone.borrow_mut() = Some((pw, plaintext));\n"
        ));
        let lock_ops = read("src/ui/editor/lock_ops.rs");
        assert!(lock_ops.contains("            Ok(plaintext) => {\n                self.restore_after_unlock(pw, plaintext);\n"));
        let stego = read("src/features/stego/mod.rs");
        assert!(stego.contains("        Ok(mut plaintext) => Ok(std::mem::take(&mut *plaintext)),\n"));
        let backup = read("src/features/backup/mod.rs");
        assert!(backup.contains("    let compressed: Zeroizing<String> =\n        crate::crypto::decrypt(&bytes[HEADER_LEN..], password).map_err("));
        assert!(backup.contains("    let plain: Zeroizing<String> =\n        crate::crypto::decrypt(&encrypted, password).map_err("));
        // Every production call site is one of those (tests excepted).
        let calls: usize = ["src/ui/dialogs/unlock.rs", "src/ui/editor/lock_ops.rs", "src/features/stego/mod.rs", "src/features/backup/mod.rs", "src/ui/mod.rs"]
            .iter()
            .map(|p| {
                let src = read(p);
                let code = src.split("#[cfg(test)]").next().unwrap().to_string();
                code.matches("crypto::decrypt(").count()
            })
            .sum();
        assert_eq!(calls, 5);
    }
}
