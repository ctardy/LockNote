// SSH keys — generation, vault storage, Windows ssh-agent (issue 054)
//
// Pure part (this file): key generation and OpenSSH serialization, public
// line and SHA-256 fingerprint, vault entries, ssh-agent message encoding and
// decoding (draft-miller-ssh-agent). The named-pipe transport to
// `\\.\pipe\openssh-ssh-agent` lives apart so everything here is testable.
//
// A generated key is stored in the vault as two secrets of category
// `SSH Keys`: `<name>` (OpenSSH private key, encrypted when a passphrase is
// given) and `<name>.pub` (public line).

use std::fmt;
use std::io::Read;

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ssh_encoding::Encode as _;
use ssh_key::private::{EcdsaKeypair, Ed25519Keypair, KeypairData, RsaKeypair};
use ssh_key::rand_core::OsRng;
use ssh_key::{EcdsaCurve, HashAlg, LineEnding, PrivateKey, PublicKey};
use zeroize::Zeroizing;

use crate::features::vault::SecretEntry;

#[cfg(windows)]
pub mod win;

/// Vault category holding SSH keys (created on first save if missing).
pub const VAULT_CATEGORY: &str = "SSH Keys";

/// Ceiling on one ssh-agent frame. An identities answer with a few dozen RSA
/// keys stays far below; anything larger is treated as a protocol error.
pub const MAX_AGENT_FRAME: usize = 256 * 1024;

// ssh-agent message numbers (draft-miller-ssh-agent, section 6.1).
pub const SSH_AGENT_FAILURE: u8 = 5;
pub const SSH_AGENT_SUCCESS: u8 = 6;
pub const SSH2_AGENTC_REQUEST_IDENTITIES: u8 = 11;
pub const SSH2_AGENT_IDENTITIES_ANSWER: u8 = 12;
pub const SSH2_AGENTC_ADD_IDENTITY: u8 = 17;
pub const SSH2_AGENTC_REMOVE_IDENTITY: u8 = 18;

/// Key types offered by the generator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyType {
    Ed25519,
    EcdsaP256,
    EcdsaP384,
    Rsa3072,
    Rsa4096,
}

impl KeyType {
    pub const ALL: [KeyType; 5] = [
        KeyType::Ed25519,
        KeyType::EcdsaP256,
        KeyType::EcdsaP384,
        KeyType::Rsa3072,
        KeyType::Rsa4096,
    ];

    /// SSH key type name, first field of the public line.
    pub fn algorithm_name(self) -> &'static str {
        match self {
            KeyType::Ed25519 => "ssh-ed25519",
            KeyType::EcdsaP256 => "ecdsa-sha2-nistp256",
            KeyType::EcdsaP384 => "ecdsa-sha2-nistp384",
            KeyType::Rsa3072 | KeyType::Rsa4096 => "ssh-rsa",
        }
    }
}

/// A freshly generated key pair. The private part is wiped on drop.
pub struct GeneratedKey {
    /// OpenSSH private key (`-----BEGIN OPENSSH PRIVATE KEY-----`),
    /// encrypted when a passphrase was given.
    pub private_openssh: Zeroizing<String>,
    /// `<algorithm> <base64 blob>[ <comment>]`
    pub public_line: String,
    /// `SHA256:<base64 without padding>`
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SshKeyError {
    /// Key generation failed (RNG, library error).
    Generation(String),
    /// Malformed private key, public line or key blob.
    InvalidKey(String),
    /// Comment with a line break or control character: it would break the
    /// public line and the `agent_key=` session line.
    InvalidComment,
    /// Vault entry name empty after trimming.
    EmptyName,
    /// Vault entry name with `[`, `]` or a control character: the vault
    /// header `[SECRET:name:category]` would not read it back.
    InvalidName,
    /// A vault entry with this exact name already exists.
    DuplicateName(String),
    /// The private key is encrypted and no passphrase was given.
    PassphraseRequired,
    /// The passphrase does not decrypt the private key.
    WrongPassphrase,
    /// The agent answered SSH_AGENT_FAILURE.
    AgentFailure,
    /// Truncated, oversized or unexpected agent message.
    AgentProtocol(String),
    /// The agent pipe does not exist: the ssh-agent service is not running.
    AgentUnavailable,
    /// The agent did not answer in time.
    AgentTimeout,
    /// File or system call failure (export, pipe I/O).
    Io(String),
}

impl fmt::Display for SshKeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SshKeyError::Generation(e) => write!(f, "key generation failed: {}", e),
            SshKeyError::InvalidKey(e) => write!(f, "invalid key: {}", e),
            SshKeyError::InvalidComment => write!(f, "comment must not contain line breaks or control characters"),
            SshKeyError::EmptyName => write!(f, "name cannot be empty"),
            SshKeyError::InvalidName => write!(f, "name must not contain [ ] or control characters"),
            SshKeyError::DuplicateName(n) => write!(f, "a secret named '{}' already exists", n),
            SshKeyError::PassphraseRequired => write!(f, "this key is encrypted: passphrase required"),
            SshKeyError::WrongPassphrase => write!(f, "wrong passphrase"),
            SshKeyError::AgentFailure => write!(f, "ssh-agent refused the operation"),
            SshKeyError::AgentProtocol(e) => write!(f, "ssh-agent protocol error: {}", e),
            SshKeyError::AgentUnavailable => write!(f, "the Windows ssh-agent service is not running"),
            SshKeyError::AgentTimeout => write!(f, "ssh-agent did not answer in time"),
            SshKeyError::Io(e) => write!(f, "{}", e),
        }
    }
}

/// Generate a key pair. An empty `passphrase` leaves the private key
/// unencrypted.
pub fn generate(kind: KeyType, comment: &str, passphrase: &str) -> Result<GeneratedKey, SshKeyError> {
    if comment.chars().any(char::is_control) {
        return Err(SshKeyError::InvalidComment);
    }
    let mut rng = OsRng;
    let gen_err = |e: ssh_key::Error| SshKeyError::Generation(e.to_string());
    let key_data = match kind {
        KeyType::Ed25519 => KeypairData::from(Ed25519Keypair::random(&mut rng)),
        KeyType::EcdsaP256 => KeypairData::from(EcdsaKeypair::random(&mut rng, EcdsaCurve::NistP256).map_err(gen_err)?),
        KeyType::EcdsaP384 => KeypairData::from(EcdsaKeypair::random(&mut rng, EcdsaCurve::NistP384).map_err(gen_err)?),
        KeyType::Rsa3072 => KeypairData::from(RsaKeypair::random(&mut rng, 3072).map_err(gen_err)?),
        KeyType::Rsa4096 => KeypairData::from(RsaKeypair::random(&mut rng, 4096).map_err(gen_err)?),
    };
    let mut key = PrivateKey::new(key_data, comment).map_err(gen_err)?;
    let public_line = key.public_key().to_openssh().map_err(gen_err)?;
    let fingerprint = key.fingerprint(HashAlg::Sha256).to_string();
    if !passphrase.is_empty() {
        key = key.encrypt(&mut rng, passphrase).map_err(gen_err)?;
    }
    let private_openssh = key.to_openssh(LineEnding::LF).map_err(gen_err)?;
    Ok(GeneratedKey { private_openssh, public_line, fingerprint })
}

/// Parse an OpenSSH private key and decrypt it when needed.
fn open_private(private_openssh: &str, passphrase: &str) -> Result<PrivateKey, SshKeyError> {
    let key = PrivateKey::from_openssh(private_openssh.trim())
        .map_err(|e| SshKeyError::InvalidKey(e.to_string()))?;
    if !key.is_encrypted() {
        return Ok(key);
    }
    if passphrase.is_empty() {
        return Err(SshKeyError::PassphraseRequired);
    }
    // A wrong passphrase decrypts to garbage: the two check integers differ
    // and ssh-key reports Error::Crypto. Anything else is a damaged key.
    key.decrypt(passphrase).map_err(|e| match e {
        ssh_key::Error::Crypto => SshKeyError::WrongPassphrase,
        other => SshKeyError::InvalidKey(other.to_string()),
    })
}

/// Public line (with the key's comment) of an OpenSSH private key.
pub fn public_line_from_private(private_openssh: &str, passphrase: &str) -> Result<String, SshKeyError> {
    let key = open_private(private_openssh, passphrase)?;
    key.public_key().to_openssh().map_err(|e| SshKeyError::InvalidKey(e.to_string()))
}

/// `SHA256:` fingerprint of a public line, as printed by `ssh-keygen -l`.
pub fn fingerprint(public_line: &str) -> Result<String, SshKeyError> {
    let key = PublicKey::from_openssh(public_line.trim())
        .map_err(|e| SshKeyError::InvalidKey(e.to_string()))?;
    Ok(key.fingerprint(HashAlg::Sha256).to_string())
}

/// Key blob (second field, base64-decoded) of a public line.
fn line_blob(public_line: &str) -> Option<Vec<u8>> {
    let b64 = public_line.split_whitespace().nth(1)?;
    BASE64.decode(b64).ok()
}

/// True when both public lines carry the same key (comments ignored).
/// Compares the raw key blobs, so it also works for key types this crate
/// cannot parse (certificates, security keys) listed by the agent.
pub fn same_key(a: &str, b: &str) -> bool {
    match (line_blob(a), line_blob(b)) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

/// The line kept in the session state for a key loaded into the agent:
/// `<type> <base64 blob>`, comment dropped. The comment is free text (and
/// unchecked for imported keys): it could hold `[/LOCKNOTE_STATE]` or a line
/// break and break the session block. `same_key` ignores comments anyway.
pub fn agent_key_line(public_line: &str) -> Result<String, SshKeyError> {
    let mut fields = public_line.split_whitespace();
    let (Some(kind), Some(b64)) = (fields.next(), fields.next()) else {
        return Err(SshKeyError::InvalidKey("public line needs a type and a key".into()));
    };
    let blob = BASE64.decode(b64).map_err(|e| SshKeyError::InvalidKey(e.to_string()))?;
    let key = PublicKey::from_bytes(&blob).map_err(|e| SshKeyError::InvalidKey(e.to_string()))?;
    if key.algorithm().as_str() != kind {
        return Err(SshKeyError::InvalidKey(format!("type {} does not match the key", kind)));
    }
    Ok(format!("{} {}", kind, BASE64.encode(&blob)))
}

/// After a failed ADD_IDENTITY: true when the agent certainly did not take
/// the key. Otherwise (timeout, I/O, garbled answer) the request may have
/// reached the agent, so the caller must track the key as loaded: removal
/// drops it later if it turns out to be absent.
pub fn add_error_is_definitive(e: &SshKeyError) -> bool {
    matches!(
        e,
        SshKeyError::AgentFailure
            | SshKeyError::AgentUnavailable
            | SshKeyError::PassphraseRequired
            | SshKeyError::WrongPassphrase
            | SshKeyError::InvalidKey(_)
    )
}

/// The two vault secrets for `key`: `<name>` (private) and `<name>.pub`.
/// Fails if either name is already used in `existing`.
pub fn vault_entries(name: &str, key: &GeneratedKey, existing: &[SecretEntry]) -> Result<[SecretEntry; 2], SshKeyError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(SshKeyError::EmptyName);
    }
    if name.chars().any(|c| c == '[' || c == ']' || c.is_control()) {
        return Err(SshKeyError::InvalidName);
    }
    let public_name = format!("{}.pub", name);
    for wanted in [name, public_name.as_str()] {
        if existing.iter().any(|s| s.name == wanted) {
            return Err(SshKeyError::DuplicateName(wanted.to_string()));
        }
    }
    // The vault drops a value's final newline on reload (vault::parse), so
    // the private key is stored without it: the stored value then reads back
    // byte for byte. OpenSSH accepts the key either way; export adds it back.
    Ok([
        SecretEntry::in_category(name, key.private_openssh.trim_end(), VAULT_CATEGORY),
        SecretEntry::in_category(&public_name, &key.public_line, VAULT_CATEGORY),
    ])
}

fn put_string(out: &mut Vec<u8>, s: &[u8]) {
    out.extend_from_slice(&(s.len() as u32).to_be_bytes());
    out.extend_from_slice(s);
}

/// SSH2_AGENTC_ADD_IDENTITY frame (length prefix included).
pub fn encode_add_identity(private_openssh: &str, passphrase: &str) -> Result<Zeroizing<Vec<u8>>, SshKeyError> {
    let key = open_private(private_openssh, passphrase)?;
    // Body: byte 17, then the keypair in the OpenSSH private-key layout
    // (key type, public and private fields: the layout the agent expects),
    // then the comment. No constraint: the Windows agent refuses them.
    let key_len = key.key_data().encoded_len().map_err(|e| SshKeyError::InvalidKey(e.to_string()))?;
    let comment = key.comment().as_bytes();
    // Sized up front: a reallocation would free a copy of the private key
    // without wiping it.
    let mut body = Zeroizing::new(Vec::with_capacity(1 + key_len + 4 + comment.len()));
    body.push(SSH2_AGENTC_ADD_IDENTITY);
    key.key_data()
        .encode(&mut *body)
        .map_err(|e| SshKeyError::InvalidKey(e.to_string()))?;
    put_string(&mut body, comment);
    let mut out = Zeroizing::new(Vec::with_capacity(4 + body.len()));
    put_string(&mut out, &body);
    Ok(out)
}

/// SSH2_AGENTC_REMOVE_IDENTITY frame (length prefix included).
pub fn encode_remove_identity(public_line: &str) -> Result<Vec<u8>, SshKeyError> {
    let key = PublicKey::from_openssh(public_line.trim())
        .map_err(|e| SshKeyError::InvalidKey(e.to_string()))?;
    let blob = key.to_bytes().map_err(|e| SshKeyError::InvalidKey(e.to_string()))?;
    let mut body = vec![SSH2_AGENTC_REMOVE_IDENTITY];
    put_string(&mut body, &blob);
    let mut out = Vec::with_capacity(4 + body.len());
    put_string(&mut out, &body);
    Ok(out)
}

/// SSH2_AGENTC_REQUEST_IDENTITIES frame (length prefix included).
pub fn encode_request_identities() -> Vec<u8> {
    vec![0, 0, 0, 1, SSH2_AGENTC_REQUEST_IDENTITIES]
}

/// Failure codes, as OpenSSH's `agent_failed()` (authfd.c) accepts them:
/// SSH_AGENT_FAILURE, SSH2_AGENT_FAILURE and SSH_COM_AGENT2_FAILURE.
fn is_failure(code: u8) -> bool {
    matches!(code, SSH_AGENT_FAILURE | 30 | 102)
}

/// Decode a SUCCESS / FAILURE answer body (length prefix removed).
pub fn decode_status(body: &[u8]) -> Result<(), SshKeyError> {
    match body {
        [SSH_AGENT_SUCCESS] => Ok(()),
        [code] if is_failure(*code) => Err(SshKeyError::AgentFailure),
        [code] => Err(SshKeyError::AgentProtocol(format!("unexpected answer type {}", code))),
        [] => Err(SshKeyError::AgentProtocol("empty answer".into())),
        _ => Err(SshKeyError::AgentProtocol(format!("unexpected answer of {} bytes", body.len()))),
    }
}

/// Bounds-checked reader over an agent message body.
struct WireReader<'a> {
    buf: &'a [u8],
}

impl<'a> WireReader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], SshKeyError> {
        if n > self.buf.len() {
            return Err(SshKeyError::AgentProtocol("truncated answer".into()));
        }
        let (head, tail) = self.buf.split_at(n);
        self.buf = tail;
        Ok(head)
    }
    fn u32(&mut self) -> Result<u32, SshKeyError> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn string(&mut self) -> Result<&'a [u8], SshKeyError> {
        let n = self.u32()? as usize;
        self.take(n)
    }
}

/// Decode an SSH2_AGENT_IDENTITIES_ANSWER body into public lines
/// (`<type> <base64 blob>[ <comment>]`). The line is rebuilt from the raw
/// blob, so any key type the agent holds is listed.
pub fn decode_identities(body: &[u8]) -> Result<Vec<String>, SshKeyError> {
    let mut r = WireReader { buf: body };
    let kind = r.take(1).map_err(|_| SshKeyError::AgentProtocol("empty answer".into()))?[0];
    if is_failure(kind) {
        return Err(SshKeyError::AgentFailure);
    }
    if kind != SSH2_AGENT_IDENTITIES_ANSWER {
        return Err(SshKeyError::AgentProtocol(format!("unexpected answer type {}", kind)));
    }
    let count = r.u32()? as usize;
    // Each identity takes at least 8 bytes (two empty strings).
    if count > r.buf.len() / 8 {
        return Err(SshKeyError::AgentProtocol(format!("identity count {} exceeds the answer size", count)));
    }
    let mut lines = Vec::with_capacity(count);
    for _ in 0..count {
        let blob = r.string()?;
        let comment = String::from_utf8_lossy(r.string()?).into_owned();
        let type_name = WireReader { buf: blob }
            .string()
            .ok()
            .and_then(|t| std::str::from_utf8(t).ok())
            .filter(|t| !t.is_empty() && !t.chars().any(|c| c.is_whitespace() || c.is_control()))
            .ok_or_else(|| SshKeyError::AgentProtocol("malformed key blob".into()))?;
        let mut line = format!("{} {}", type_name, BASE64.encode(blob));
        if !comment.is_empty() {
            line.push(' ');
            line.push_str(&comment);
        }
        lines.push(line);
    }
    if !r.buf.is_empty() {
        return Err(SshKeyError::AgentProtocol("trailing data after identities".into()));
    }
    Ok(lines)
}

/// Read one length-prefixed frame and return its body. A length above
/// MAX_AGENT_FRAME is rejected before any allocation.
pub fn read_frame(reader: &mut impl Read) -> Result<Vec<u8>, SshKeyError> {
    let io_err = |e: std::io::Error| SshKeyError::AgentProtocol(format!("read failed: {}", e));
    let mut len = [0u8; 4];
    reader.read_exact(&mut len).map_err(io_err)?;
    let n = u32::from_be_bytes(len) as usize;
    if n == 0 {
        return Err(SshKeyError::AgentProtocol("empty frame".into()));
    }
    if n > MAX_AGENT_FRAME {
        return Err(SshKeyError::AgentProtocol(format!("frame of {} bytes exceeds the limit", n)));
    }
    let mut body = vec![0u8; n];
    reader.read_exact(&mut body).map_err(io_err)?;
    Ok(body)
}

/// The two agent requests used to unload keys (a trait so the removal
/// policy is testable without the Windows service).
pub trait Agent {
    /// Public lines of the keys the agent holds.
    fn identities(&mut self) -> Result<Vec<String>, SshKeyError>;
    /// Remove one key, given by its public line.
    fn remove(&mut self, public_line: &str) -> Result<(), SshKeyError>;
}

/// Result of `remove_loaded_keys`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct RemovalOutcome {
    /// Keys still to remove: keep them in the session state.
    pub pending: Vec<String>,
    /// Keys removed from the agent.
    pub removed: Vec<String>,
    /// Keys no longer in the agent (removed by someone else, agent restart).
    pub absent: Vec<String>,
    /// First error met; `pending` is non-empty whenever this is set.
    pub error: Option<SshKeyError>,
}

/// Remove from the agent the keys this file loaded (`loaded`, public
/// lines). The agent answers FAILURE both for "not here" and for a real
/// refusal, so the identities are listed first: an absent key is dropped
/// without a remove request. Keys that could not be removed stay pending.
/// `unconfirmed`: keys whose add timed out in this session; the request may
/// still land, so while absent they stay pending instead of being dropped.
pub fn remove_loaded_keys(agent: &mut impl Agent, loaded: &[String], unconfirmed: &[String]) -> RemovalOutcome {
    let mut out = RemovalOutcome::default();
    if loaded.is_empty() {
        return out;
    }
    let held = match agent.identities() {
        Ok(held) => held,
        Err(e) => {
            out.pending = loaded.to_vec();
            out.error = Some(e);
            return out;
        }
    };
    for key in loaded {
        if !held.iter().any(|h| same_key(h, key)) {
            if unconfirmed.iter().any(|u| same_key(u, key)) {
                out.pending.push(key.clone());
            } else {
                out.absent.push(key.clone());
            }
            continue;
        }
        match agent.remove(key) {
            Ok(()) => out.removed.push(key.clone()),
            Err(e) => {
                out.pending.push(key.clone());
                out.error.get_or_insert(e);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD};
    use sha2::{Digest, Sha256};

    // --- helpers: minimal SSH wire reader / writer for checking frames ---

    struct Wire<'a> {
        buf: &'a [u8],
    }

    impl<'a> Wire<'a> {
        fn u8(&mut self) -> u8 {
            let b = self.buf[0];
            self.buf = &self.buf[1..];
            b
        }
        fn u32(&mut self) -> u32 {
            let v = u32::from_be_bytes(self.buf[..4].try_into().unwrap());
            self.buf = &self.buf[4..];
            v
        }
        fn string(&mut self) -> &'a [u8] {
            let n = self.u32() as usize;
            let s = &self.buf[..n];
            self.buf = &self.buf[n..];
            s
        }
        fn is_empty(&self) -> bool {
            self.buf.is_empty()
        }
    }

    fn put_string(out: &mut Vec<u8>, s: &[u8]) {
        out.extend_from_slice(&(s.len() as u32).to_be_bytes());
        out.extend_from_slice(s);
    }

    /// Body of a framed message, after checking the length prefix.
    fn body(frame: &[u8]) -> &[u8] {
        let n = u32::from_be_bytes(frame[..4].try_into().unwrap()) as usize;
        assert_eq!(n, frame.len() - 4, "length prefix must match the body");
        &frame[4..]
    }

    fn blob_of(public_line: &str) -> Vec<u8> {
        let b64 = public_line.split(' ').nth(1).expect("base64 field");
        STANDARD.decode(b64).expect("valid base64")
    }

    fn ed25519(comment: &str, passphrase: &str) -> GeneratedKey {
        generate(KeyType::Ed25519, comment, passphrase).expect("ed25519 generation")
    }

    // --- generation and serialization ---

    #[test]
    fn generate_each_algorithm_roundtrip() {
        for kind in KeyType::ALL {
            let k = generate(kind, "test@locknote", "").unwrap_or_else(|e| panic!("{:?}: {}", kind, e));
            assert!(
                k.private_openssh.starts_with("-----BEGIN OPENSSH PRIVATE KEY-----"),
                "{:?}: not an OpenSSH private key", kind
            );
            assert!(k.public_line.starts_with(&format!("{} ", kind.algorithm_name())), "{:?}", kind);
            assert!(k.public_line.ends_with(" test@locknote"), "{:?}", kind);
            assert_eq!(public_line_from_private(&k.private_openssh, "").unwrap(), k.public_line, "{:?}", kind);
            assert_eq!(fingerprint(&k.public_line).unwrap(), k.fingerprint, "{:?}", kind);
        }
    }

    #[test]
    fn two_generations_give_different_keys() {
        let a = ed25519("", "");
        let b = ed25519("", "");
        assert!(!same_key(&a.public_line, &b.public_line));
    }

    #[test]
    fn encrypted_private_key_requires_passphrase() {
        let k = ed25519("enc", "correct horse");
        assert_eq!(public_line_from_private(&k.private_openssh, "correct horse").unwrap(), k.public_line);
        assert_eq!(public_line_from_private(&k.private_openssh, "wrong"), Err(SshKeyError::WrongPassphrase));
        assert_eq!(public_line_from_private(&k.private_openssh, ""), Err(SshKeyError::PassphraseRequired));
        assert!(encode_add_identity(&k.private_openssh, "correct horse").is_ok());
        assert_eq!(encode_add_identity(&k.private_openssh, "wrong").err(), Some(SshKeyError::WrongPassphrase));
        assert_eq!(encode_add_identity(&k.private_openssh, "").err(), Some(SshKeyError::PassphraseRequired));
    }

    #[test]
    fn malformed_private_key_is_rejected() {
        assert!(matches!(public_line_from_private("not a key", ""), Err(SshKeyError::InvalidKey(_))));
        assert!(matches!(encode_add_identity("", ""), Err(SshKeyError::InvalidKey(_))));
    }

    #[test]
    fn public_line_format() {
        let k = ed25519("alice@pc", "");
        let parts: Vec<&str> = k.public_line.split(' ').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], "ssh-ed25519");
        assert_eq!(parts[2], "alice@pc");
        // Blob: string "ssh-ed25519" + string pubkey(32)
        let blob = blob_of(&k.public_line);
        let mut w = Wire { buf: &blob };
        assert_eq!(w.string(), b"ssh-ed25519");
        assert_eq!(w.string().len(), 32);
        assert!(w.is_empty());

        // Empty comment: two fields, no trailing space.
        let k = ed25519("", "");
        assert_eq!(k.public_line.split(' ').count(), 2);
        assert!(!k.public_line.ends_with(' '));
    }

    #[test]
    fn comment_with_line_break_is_rejected() {
        for bad in ["a\nb", "a\rb", "a\u{0}b", "a\tb"] {
            assert_eq!(generate(KeyType::Ed25519, bad, "").err(), Some(SshKeyError::InvalidComment), "{:?}", bad);
        }
    }

    #[test]
    fn fingerprint_sha256_format() {
        let k = ed25519("x", "");
        let expected = format!("SHA256:{}", STANDARD_NO_PAD.encode(Sha256::digest(blob_of(&k.public_line))));
        assert_eq!(k.fingerprint, expected);
        assert_eq!(k.fingerprint.len(), "SHA256:".len() + 43);
        assert!(!k.fingerprint.contains('='));
        assert!(matches!(fingerprint("garbage"), Err(SshKeyError::InvalidKey(_))));
        assert!(matches!(fingerprint("ssh-ed25519 !!!notbase64"), Err(SshKeyError::InvalidKey(_))));
    }

    #[test]
    fn same_key_ignores_comment() {
        let k = ed25519("one", "");
        let no_comment: String = k.public_line.split(' ').take(2).collect::<Vec<_>>().join(" ");
        assert!(same_key(&k.public_line, &no_comment));
        assert!(same_key(&k.public_line, &format!("{} other comment", no_comment)));
        assert!(!same_key(&k.public_line, "garbage"));
    }

    // --- vault ---

    #[test]
    fn vault_entries_for_key() {
        let k = ed25519("c", "");
        let [private, public] = vault_entries("id_ed25519", &k, &[]).unwrap();
        assert_eq!(private.name, "id_ed25519");
        assert_eq!(private.value, k.private_openssh.trim_end());
        assert_eq!(private.category, VAULT_CATEGORY);
        assert_eq!(public.name, "id_ed25519.pub");
        assert_eq!(public.value, k.public_line);
        assert_eq!(public.category, VAULT_CATEGORY);

        // Name is trimmed.
        let [private, _] = vault_entries("  work  ", &k, &[]).unwrap();
        assert_eq!(private.name, "work");
    }

    #[test]
    fn vault_entries_refuse_duplicates_and_empty_name() {
        let k = ed25519("c", "");
        let existing = vec![SecretEntry::in_category("id_ed25519.pub", "x", "General")];
        assert_eq!(
            vault_entries("id_ed25519", &k, &existing).err(),
            Some(SshKeyError::DuplicateName("id_ed25519.pub".into()))
        );
        let existing = vec![SecretEntry::in_category("id_ed25519", "x", VAULT_CATEGORY)];
        assert_eq!(
            vault_entries("id_ed25519", &k, &existing).err(),
            Some(SshKeyError::DuplicateName("id_ed25519".into()))
        );
        assert_eq!(vault_entries("   ", &k, &[]).err(), Some(SshKeyError::EmptyName));
    }

    #[test]
    fn vault_entries_refuse_names_the_vault_cannot_read_back() {
        let k = ed25519("c", "");
        for bad in ["a]b", "[x", "a\nb", "a\tb"] {
            assert_eq!(vault_entries(bad, &k, &[]).err(), Some(SshKeyError::InvalidName), "{:?}", bad);
        }
        assert!(vault_entries("work:laptop", &k, &[]).is_ok(), "':' is fine (rfind in vault parse)");
    }

    #[test]
    fn agent_key_line_drops_the_comment() {
        let k = ed25519("", "");
        let bare = k.public_line.clone();
        for comment in ["x [/LOCKNOTE_STATE]", "x [LOCKNOTE_STATE]", "a\nactive=99", "plain"] {
            let line = format!("{} {}", bare, comment);
            assert_eq!(agent_key_line(&line).unwrap(), bare, "{:?}", comment);
        }
        assert_eq!(agent_key_line(&bare).unwrap(), bare);
        assert!(matches!(agent_key_line("garbage"), Err(SshKeyError::InvalidKey(_))));
        assert!(matches!(agent_key_line("ssh-ed25519 !!!"), Err(SshKeyError::InvalidKey(_))));
        // Type field must match the blob.
        let lying = bare.replacen("ssh-ed25519", "ssh-rsa", 1);
        assert!(matches!(agent_key_line(&lying), Err(SshKeyError::InvalidKey(_))));
    }

    #[test]
    fn hostile_comment_cannot_break_the_session_block() {
        use crate::features::session_state::{combine, parse, SessionState};
        let k = ed25519("", "");
        let hostile = format!("{} x [/LOCKNOTE_STATE]\nactive=99", k.public_line);
        let state = SessionState { active_tab: 1, agent_keys: vec![agent_key_line(&hostile).unwrap()], ..Default::default() };
        let (parsed, rest) = parse(&combine("note", &state));
        assert_eq!(parsed, Some(state));
        assert_eq!(rest, "note");
    }

    #[test]
    fn add_errors_that_may_have_reached_the_agent_are_not_definitive() {
        for e in [SshKeyError::AgentTimeout, SshKeyError::Io("x".into()), SshKeyError::AgentProtocol("x".into())] {
            assert!(!add_error_is_definitive(&e), "{:?}", e);
        }
        for e in [
            SshKeyError::AgentFailure,
            SshKeyError::AgentUnavailable,
            SshKeyError::PassphraseRequired,
            SshKeyError::WrongPassphrase,
            SshKeyError::InvalidKey("x".into()),
        ] {
            assert!(add_error_is_definitive(&e), "{:?}", e);
        }
    }

    #[test]
    fn vault_value_roundtrip_pem() {
        for passphrase in ["", "pp"] {
            let k = ed25519("roundtrip", passphrase);
            let entries = vault_entries("id_ed25519", &k, &[]).unwrap();
            let text = crate::features::vault::serialize(&entries);
            let (parsed, rest) = crate::features::vault::parse_with_categories(&text, &[VAULT_CATEGORY.to_string()]);
            assert_eq!(rest, "");
            assert_eq!(parsed, entries.to_vec());
            assert_eq!(public_line_from_private(&parsed[0].value, passphrase).unwrap(), k.public_line);
        }
    }

    // --- ssh-agent protocol ---

    #[test]
    fn agent_add_identity_encoding_ed25519() {
        let k = ed25519("agent", "");
        let frame = encode_add_identity(&k.private_openssh, "").unwrap();
        let mut w = Wire { buf: body(&frame) };
        assert_eq!(w.u8(), SSH2_AGENTC_ADD_IDENTITY);
        assert_eq!(w.string(), b"ssh-ed25519");
        let public = w.string().to_vec();
        assert_eq!(public.len(), 32);
        let private = w.string();
        assert_eq!(private.len(), 64, "seed (32) + public key (32)");
        assert_eq!(&private[32..], &public[..]);
        assert_eq!(w.string(), b"agent");
        assert!(w.is_empty(), "no constraint appended");

        // Same public key as the public line.
        let mut b = Wire { buf: &blob_of(&k.public_line) };
        b.string();
        assert_eq!(b.string(), &public[..]);
    }

    #[test]
    fn agent_add_identity_encoding_rsa() {
        let k = generate(KeyType::Rsa3072, "rsa", "").unwrap();
        let frame = encode_add_identity(&k.private_openssh, "").unwrap();
        let mut w = Wire { buf: body(&frame) };
        assert_eq!(w.u8(), SSH2_AGENTC_ADD_IDENTITY);
        assert_eq!(w.string(), b"ssh-rsa");
        for _ in 0..6 {
            // n, e, d, iqmp, p, q
            assert!(!w.string().is_empty());
        }
        assert_eq!(w.string(), b"rsa");
        assert!(w.is_empty());
    }

    #[test]
    fn agent_remove_identity_encoding() {
        let k = ed25519("rm", "");
        let frame = encode_remove_identity(&k.public_line).unwrap();
        let mut w = Wire { buf: body(&frame) };
        assert_eq!(w.u8(), SSH2_AGENTC_REMOVE_IDENTITY);
        assert_eq!(w.string(), &blob_of(&k.public_line)[..]);
        assert!(w.is_empty());
        assert!(matches!(encode_remove_identity("garbage"), Err(SshKeyError::InvalidKey(_))));

        let rsa = generate(KeyType::Rsa3072, "rm", "").unwrap();
        let frame = encode_remove_identity(&rsa.public_line).unwrap();
        let mut w = Wire { buf: body(&frame) };
        assert_eq!(w.u8(), SSH2_AGENTC_REMOVE_IDENTITY);
        assert_eq!(w.string(), &blob_of(&rsa.public_line)[..]);
        assert!(w.is_empty());
    }

    #[test]
    fn agent_request_identities_encoding() {
        assert_eq!(encode_request_identities(), vec![0, 0, 0, 1, SSH2_AGENTC_REQUEST_IDENTITIES]);
    }

    #[test]
    fn agent_response_decoding() {
        assert_eq!(decode_status(&[SSH_AGENT_SUCCESS]), Ok(()));
        assert_eq!(decode_status(&[SSH_AGENT_FAILURE]), Err(SshKeyError::AgentFailure));
        assert!(matches!(decode_status(&[]), Err(SshKeyError::AgentProtocol(_))));
        assert!(matches!(decode_status(&[99]), Err(SshKeyError::AgentProtocol(_))));
        assert!(matches!(decode_status(&[SSH_AGENT_SUCCESS, 0]), Err(SshKeyError::AgentProtocol(_))));
    }

    #[test]
    fn agent_identities_decoding() {
        let k = ed25519("ignored", "");
        let blob = blob_of(&k.public_line);
        let mut msg = vec![SSH2_AGENT_IDENTITIES_ANSWER];
        msg.extend_from_slice(&1u32.to_be_bytes());
        put_string(&mut msg, &blob);
        put_string(&mut msg, b"from agent");
        let lines = decode_identities(&msg).unwrap();
        assert_eq!(lines.len(), 1);
        assert!(same_key(&lines[0], &k.public_line));
        assert!(lines[0].ends_with(" from agent"));

        // Zero identities.
        let mut empty = vec![SSH2_AGENT_IDENTITIES_ANSWER];
        empty.extend_from_slice(&0u32.to_be_bytes());
        assert_eq!(decode_identities(&empty).unwrap(), Vec::<String>::new());

        // Truncated: count says 1, nothing follows.
        let mut truncated = vec![SSH2_AGENT_IDENTITIES_ANSWER];
        truncated.extend_from_slice(&1u32.to_be_bytes());
        assert!(matches!(decode_identities(&truncated), Err(SshKeyError::AgentProtocol(_))));

        // Absurd count.
        let mut absurd = vec![SSH2_AGENT_IDENTITIES_ANSWER];
        absurd.extend_from_slice(&u32::MAX.to_be_bytes());
        assert!(matches!(decode_identities(&absurd), Err(SshKeyError::AgentProtocol(_))));

        // String length past the end.
        let mut overrun = vec![SSH2_AGENT_IDENTITIES_ANSWER];
        overrun.extend_from_slice(&1u32.to_be_bytes());
        overrun.extend_from_slice(&1000u32.to_be_bytes());
        overrun.extend_from_slice(b"short");
        assert!(matches!(decode_identities(&overrun), Err(SshKeyError::AgentProtocol(_))));

        // Key type with a space: would shift the fields of the rebuilt line.
        let mut odd_blob = Vec::new();
        put_string(&mut odd_blob, b"ssh-ed25519 x");
        put_string(&mut odd_blob, &[0u8; 32]);
        let mut odd = vec![SSH2_AGENT_IDENTITIES_ANSWER];
        odd.extend_from_slice(&1u32.to_be_bytes());
        put_string(&mut odd, &odd_blob);
        put_string(&mut odd, b"");
        assert!(matches!(decode_identities(&odd), Err(SshKeyError::AgentProtocol(_))));

        // Wrong message type.
        assert_eq!(decode_identities(&[SSH_AGENT_FAILURE]), Err(SshKeyError::AgentFailure));
        assert!(matches!(decode_identities(&[]), Err(SshKeyError::AgentProtocol(_))));
    }

    #[test]
    fn agent_frame_reading() {
        use std::io::Cursor;
        assert_eq!(read_frame(&mut Cursor::new(vec![0, 0, 0, 1, 6])).unwrap(), vec![6]);
        // Oversized length: rejected without reading or allocating the body.
        let huge = ((MAX_AGENT_FRAME + 1) as u32).to_be_bytes().to_vec();
        assert!(matches!(read_frame(&mut Cursor::new(huge)), Err(SshKeyError::AgentProtocol(_))));
        assert!(matches!(read_frame(&mut Cursor::new(vec![0xFF; 4])), Err(SshKeyError::AgentProtocol(_))));
        // Truncated body and truncated header.
        assert!(matches!(read_frame(&mut Cursor::new(vec![0, 0, 0, 5, 1])), Err(SshKeyError::AgentProtocol(_))));
        assert!(matches!(read_frame(&mut Cursor::new(vec![0, 0])), Err(SshKeyError::AgentProtocol(_))));
        // Zero-length frame: no message type.
        assert!(matches!(read_frame(&mut Cursor::new(vec![0, 0, 0, 0])), Err(SshKeyError::AgentProtocol(_))));
    }

    // --- removal policy (fake agent) ---

    #[derive(Default)]
    struct FakeAgent {
        held: Vec<String>,
        list_error: Option<SshKeyError>,
        refuse: Vec<String>,
        calls: Vec<String>,
    }

    impl Agent for FakeAgent {
        fn identities(&mut self) -> Result<Vec<String>, SshKeyError> {
            self.calls.push("list".into());
            match &self.list_error {
                Some(e) => Err(e.clone()),
                None => Ok(self.held.clone()),
            }
        }
        fn remove(&mut self, public_line: &str) -> Result<(), SshKeyError> {
            self.calls.push(format!("remove {}", public_line));
            if self.refuse.iter().any(|r| same_key(r, public_line)) {
                return Err(SshKeyError::AgentFailure);
            }
            self.held.retain(|h| !same_key(h, public_line));
            Ok(())
        }
    }

    #[test]
    fn removal_with_empty_list_does_not_contact_agent() {
        let mut agent = FakeAgent::default();
        assert_eq!(remove_loaded_keys(&mut agent, &[], &[]), RemovalOutcome::default());
        assert!(agent.calls.is_empty());
    }

    #[test]
    fn removal_drops_present_and_absent_keys() {
        let a = ed25519("a", "").public_line;
        let b = ed25519("b", "").public_line;
        let other = ed25519("not ours", "").public_line;
        // The agent lists `a` under another comment: still the same key.
        let a_in_agent = format!("{} as listed by agent", a.rsplit_once(' ').unwrap().0);
        let mut agent = FakeAgent { held: vec![a_in_agent, other.clone()], ..Default::default() };
        let out = remove_loaded_keys(&mut agent, &[a.clone(), b.clone()], &[]);
        assert_eq!(out.removed, vec![a.clone()]);
        assert_eq!(out.absent, vec![b.clone()]);
        assert!(out.pending.is_empty());
        assert_eq!(out.error, None);
        // Only our key was removed; no remove request for the absent one.
        assert_eq!(agent.held, vec![other]);
        assert_eq!(agent.calls, vec!["list".to_string(), format!("remove {}", a)]);
    }

    #[test]
    fn removal_keeps_everything_when_agent_unreachable() {
        let a = ed25519("a", "").public_line;
        let mut agent = FakeAgent { list_error: Some(SshKeyError::AgentUnavailable), ..Default::default() };
        let out = remove_loaded_keys(&mut agent, &[a.clone()], &[]);
        assert_eq!(out.pending, vec![a]);
        assert_eq!(out.error, Some(SshKeyError::AgentUnavailable));
    }

    #[test]
    fn removal_keeps_unconfirmed_absent_key() {
        // An add that timed out may still land in the agent later: while
        // unconfirmed, an absent key stays tracked instead of being dropped.
        let late = ed25519("late", "").public_line;
        let gone = ed25519("gone", "").public_line;
        let mut agent = FakeAgent::default();
        let out = remove_loaded_keys(&mut agent, &[late.clone(), gone.clone()], &[late.clone()]);
        assert_eq!(out.pending, vec![late.clone()]);
        assert_eq!(out.absent, vec![gone]);
        assert_eq!(out.error, None, "not an error: just not there yet");

        // Once it shows up, it is removed like any other key.
        agent.held.push(late.clone());
        let out = remove_loaded_keys(&mut agent, &[late.clone()], &[late.clone()]);
        assert_eq!(out.removed, vec![late]);
        assert!(out.pending.is_empty());
    }

    #[test]
    fn removal_keeps_refused_key_and_continues() {
        let a = ed25519("a", "").public_line;
        let b = ed25519("b", "").public_line;
        let mut agent = FakeAgent { held: vec![a.clone(), b.clone()], refuse: vec![a.clone()], ..Default::default() };
        let out = remove_loaded_keys(&mut agent, &[a.clone(), b.clone()], &[]);
        assert_eq!(out.pending, vec![a]);
        assert_eq!(out.removed, vec![b]);
        assert_eq!(out.error, Some(SshKeyError::AgentFailure));
    }

    // --- real Windows agent (manual: cargo test -- --ignored; needs the
    // ssh-agent service running; touches only the throwaway key it adds) ---

    #[test]
    #[ignore]
    fn interop_windows_agent_add_list_remove() {
        let k = generate(KeyType::Ed25519, "locknote-054-agent-test", "pp").unwrap();
        let mut agent = win::PipeAgent;
        assert_eq!(agent.add(&k.private_openssh, "wrong").err(), Some(SshKeyError::WrongPassphrase));
        agent.add(&k.private_openssh, "pp").unwrap();
        let held = agent.identities().unwrap();
        assert!(held.iter().any(|h| same_key(h, &k.public_line)), "key listed after add");
        assert!(held.iter().any(|h| h.ends_with(" locknote-054-agent-test")), "comment kept");

        let out = remove_loaded_keys(&mut agent, &[k.public_line.clone()], &[]);
        assert_eq!(out.removed, vec![k.public_line.clone()]);
        assert_eq!(out.error, None);
        assert!(!agent.identities().unwrap().iter().any(|h| same_key(h, &k.public_line)), "key gone");

        // Second pass: the key is absent, dropped without a remove request.
        let out = remove_loaded_keys(&mut agent, &[k.public_line.clone()], &[]);
        assert_eq!(out.absent, vec![k.public_line]);
    }

    // --- interoperability with the system ssh-keygen (manual: cargo test -- --ignored) ---

    #[test]
    #[ignore]
    fn interop_ssh_keygen_reads_our_keys() {
        let keygen = std::path::Path::new(r"C:\Windows\System32\OpenSSH\ssh-keygen.exe");
        assert!(keygen.exists(), "ssh-keygen.exe not found");
        let dir = std::env::temp_dir().join(format!("locknote-054-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (kind, passphrase) in [(KeyType::Ed25519, ""), (KeyType::Ed25519, "pp"), (KeyType::EcdsaP256, "pp"), (KeyType::Rsa3072, "pp")] {
            let k = generate(kind, "interop", passphrase).unwrap();
            let path = dir.join(format!("key-{:?}-{}", kind, passphrase.len()));
            // Through the export path: ssh-keygen refuses a key file other
            // accounts can read, so this also checks the owner-only DACL.
            win::export_key_pair(&path, &k.private_openssh, &k.public_line).unwrap();
            assert!(win::export_key_pair(&path, &k.private_openssh, &k.public_line).is_err(), "no overwrite");
            let out = std::process::Command::new(keygen)
                .args(["-y", "-P", passphrase, "-f"])
                .arg(&path)
                .output()
                .unwrap();
            assert!(out.status.success(), "{:?}: {}", kind, String::from_utf8_lossy(&out.stderr));
            let line = String::from_utf8(out.stdout).unwrap();
            assert!(same_key(line.trim(), &k.public_line), "{:?}", kind);

            let out = std::process::Command::new(keygen).args(["-l", "-f"]).arg(win::public_path(&path)).output().unwrap();
            assert!(String::from_utf8(out.stdout).unwrap().contains(&k.fingerprint), "{:?}", kind);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
