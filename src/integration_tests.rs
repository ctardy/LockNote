//! Integration tests — full encrypt/decrypt/storage chain
//!
//! These tests exercise the interaction between crypto, compression, payload,
//! storage, and settings modules without depending on NWG or any UI code.
//! Ported from LockNote 1.2.1 (issue 059) and adapted: the storage API works
//! on a staging directory and a session given by the caller, so every test
//! uses its own scratch directory and never the real %LOCALAPPDATA%\LockNote.

use crate::crypto;
use crate::settings::Settings;
use crate::storage;
use std::fs;
use std::path::PathBuf;

/// Smallest valid Argon2 parameters: keeps the tests fast.
fn encrypt(plaintext: &str, password: &str) -> Vec<u8> {
    crypto::encrypt(plaintext, password, 8, 1, 1).unwrap()
}

/// A scratch directory with a staging directory and the path of a fake exe
/// holding `program`. Removed on drop.
struct Scratch {
    root: PathBuf,
    staging: PathBuf,
    exe: PathBuf,
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn scratch(tag: &str, program: &[u8]) -> Scratch {
    let root = std::env::temp_dir().join(format!("locknote_integration_{}_{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let staging = root.join("staging");
    let app = root.join("app");
    fs::create_dir_all(&staging).unwrap();
    fs::create_dir_all(&app).unwrap();
    let exe = app.join("LockNote.exe");
    fs::write(&exe, program).unwrap();
    Scratch { root, staging, exe }
}

impl Scratch {
    /// Save `encrypted` as a session of this exe does (.tmp + companion).
    fn save(&self, encrypted: &[u8]) {
        let (mut session, _) = storage::open_session_in(&self.staging, &self.exe).unwrap();
        storage::write_data_in(&self.staging, &mut session, encrypted).unwrap();
    }

    /// What "Import from another LockNote" reads for this exe.
    fn read(&self) -> Option<Vec<u8>> {
        storage::read_data_in(&self.staging, &self.exe).unwrap()
    }

    fn tmp(&self) -> PathBuf {
        storage::get_tmp_path_in(&self.staging, &self.exe)
    }
}

#[test]
fn full_chain_encrypt_settings_decrypt() {
    // Create settings + note, serialize, compress, encrypt, decrypt, parse back
    let settings = Settings::default();
    let note = "My secret note with special chars: <>&\"' and unicode: café 🔒";
    let combined = crate::features::compression::compress(&settings.serialize(note));

    let password = "test_password_123";
    let encrypted = encrypt(&combined, password);
    let decrypted = crate::features::compression::decompress(&crypto::decrypt(&encrypted, password).unwrap());

    let (parsed_settings, parsed_note) = Settings::parse(&decrypted);
    assert_eq!(parsed_settings.theme, settings.theme);
    assert_eq!(parsed_note, note);
}

#[test]
fn full_chain_password_change() {
    let plaintext = "sensitive data";
    let pw1 = "old_password";
    let pw2 = "new_password";

    // Encrypt with pw1
    let encrypted1 = encrypt(plaintext, pw1);
    assert!(crypto::decrypt(&encrypted1, pw1).is_ok());

    // Decrypt with pw1, re-encrypt with pw2
    let decrypted = crypto::decrypt(&encrypted1, pw1).unwrap();
    let encrypted2 = encrypt(&decrypted, pw2);

    // pw1 no longer works on new ciphertext
    assert!(crypto::decrypt(&encrypted2, pw1).is_err());
    // pw2 works
    assert_eq!(crypto::decrypt(&encrypted2, pw2).unwrap(), plaintext);
}

#[test]
fn full_chain_storage_write_read_decrypt() {
    let s = scratch("write_read", &[0x4D, 0x5A, 0x90, 0x00]);

    // Build the payload like a save does, encrypt and write
    let password = "integration_test_pw";
    let tabs = vec![
        crate::features::tabs::NoteTab::with_content("Integration", "Integration test note", None),
        crate::features::tabs::NoteTab::with_content("Second", "second tab", Some("blue")),
    ];
    let note = crate::features::payload::compose(&tabs, &[], &[], &[], &Default::default());
    let combined = crate::features::compression::compress(&Settings::default().serialize(&note));
    let encrypted = encrypt(&combined, password);
    s.save(&encrypted);

    // Read back (the .tmp is the latest state) and decrypt
    let read_back = s.read().expect("notes in the .tmp");
    assert_eq!(read_back, encrypted);

    let decrypted = crate::features::compression::decompress(&crypto::decrypt(&read_back, password).unwrap());
    let (_, parsed_note) = Settings::parse(&decrypted);
    let parsed = crate::features::payload::parse_payload(&parsed_note, &crate::features::vault::categories::effective(&[]));
    let got: Vec<(String, String)> = parsed.tabs.iter().map(|t| (t.name.clone(), t.content.clone())).collect();
    assert_eq!(
        got,
        vec![
            ("Integration".to_string(), "Integration test note".to_string()),
            ("Second".to_string(), "second tab".to_string()),
        ]
    );
}

#[test]
fn full_chain_corrupted_storage_fails_gracefully() {
    let s = scratch("corrupt", &[0x4D, 0x5A]);
    s.save(&encrypt("test", "pass"));

    // Corrupt the tmp file: flip bytes in the encrypted payload area (near the end)
    let mut data = fs::read(s.tmp()).unwrap();
    let len = data.len();
    assert!(len > 10);
    data[len - 5] ^= 0xFF;
    fs::write(s.tmp(), &data).unwrap();

    // Read back — should get corrupted data
    let read_back = s.read().expect("notes in the .tmp");
    // Decrypt should fail (HMAC mismatch)
    assert!(matches!(crypto::decrypt(&read_back, "pass"), Err(crypto::CryptoError::HmacMismatch)));
}

#[test]
fn full_chain_empty_note() {
    let settings = Settings::default();
    let combined = settings.serialize("");
    let encrypted = encrypt(&combined, "pw");
    let decrypted = crypto::decrypt(&encrypted, "pw").unwrap();
    let (_, note) = Settings::parse(&decrypted);
    assert_eq!(note, "");
}

#[test]
fn full_chain_large_note() {
    let large_note = "x".repeat(500_000);
    let settings = Settings::default();
    let combined = crate::features::compression::compress(&settings.serialize(&large_note));
    let encrypted = encrypt(&combined, "pw");
    let decrypted = crate::features::compression::decompress(&crypto::decrypt(&encrypted, "pw").unwrap());
    let (_, note) = Settings::parse(&decrypted);
    assert_eq!(note, large_note);
}

#[test]
fn storage_marker_preserved_across_updates() {
    // Simulates an update: the updater stages a new program behind which the
    // notes of the session are kept.
    let s = scratch("update", &[0xAAu8; 100]);
    let payload = encrypt("original note", "pw");
    let (mut session, _) = storage::open_session_in(&s.staging, &s.exe).unwrap();
    storage::write_data_in(&s.staging, &mut session, &payload).unwrap();

    // "New" program (different size)
    let program_v2 = vec![0xBBu8; 150];
    storage::write_staged_update_in(&s.staging, &mut session, &program_v2).unwrap();

    // The staged file is the new program followed by the same notes
    let staged = fs::read(s.tmp()).unwrap();
    let marker = storage::find_marker(&staged).expect("marker after the new program");
    assert_eq!(&staged[..marker], &program_v2[..]);

    // Verify data is still accessible
    let recovered = s.read().expect("notes in the .tmp");
    assert_eq!(recovered, payload);
    let decrypted = crypto::decrypt(&recovered, "pw").unwrap();
    assert_eq!(decrypted, "original note");
}

/// Issue #42 characterisation (written and green before the change): a save
/// with every block and both kinds of secret in the settings, written the way
/// `save_note` chains it (compose, serialize, compress, encrypt, storage),
/// then read back the way an unlock does (storage, decrypt, decompress,
/// parse, parse_payload). The plaintext bytes up to the encryption are pinned.
#[test]
fn issue42_full_exe_payload_with_every_block_round_trips() {
    use crate::features::attachments::Attachment;
    use crate::features::prompts::PromptEntry;
    use crate::features::session_state::SessionState;
    use crate::features::tabs::NoteTab;
    use crate::features::vault::SecretEntry;
    use sha2::Digest;

    let s = scratch("issue42_every_block", &[0x4D, 0x5A, 0x90, 0x00]);
    let mut settings = Settings::default();
    settings.totp_2fa = true;
    settings.totp_2fa_secret = "JBSWY3DPEHPK3PXP".into();
    settings.totp_entries = vec![crate::settings::TotpEntry { name: "gh".into(), secret: "GEZDGNBVGY3TQOJQ".into() }];
    settings.vault_categories = vec!["Email".into(), "Custom".into()];
    let tabs = vec![
        NoteTab::with_content("Note 1", "first\r\n[LOCKNOTE_VAULT]\nlast \u{e9}", None),
        NoteTab::with_content("Work", "w", Some("blue")),
    ];
    let prompts = vec![PromptEntry::new("P", "do:\n[/PROMPT]", "Writing")];
    let secrets = vec![
        SecretEntry::in_category("gmail", "p@ss\n[/SECRET]", "Email"),
        SecretEntry::in_category("c", "v", "Custom"),
    ];
    let atts = vec![Attachment::new("a.bin", (0..=255u8).cycle().take(1000).collect())];
    let state = SessionState { active_tab: 1, cursor_line: 3, agent_keys: vec!["ssh-ed25519 AAAA k".into()] };

    let note = crate::features::payload::compose(&tabs, &prompts, &secrets, &atts, &state);
    let combined = settings.serialize(&note);
    let compressed = crate::features::compression::compress(&combined);
    let hex = |b: &[u8]| -> String { sha2::Sha256::digest(b).iter().map(|x| format!("{:02x}", x)).collect() };
    assert_eq!((combined.len(), hex(combined.as_bytes())), (2525, "fbc3375914265ad0b31647419b930028696c7aec196ec0a15bd5069019cd277d".to_string()));
    assert_eq!((compressed.len(), hex(compressed.as_bytes())), (4096, "aeb0ada2b376a51bd48b455b9c738f79962208c8a1bc86941746ce5a0acba30a".to_string()));

    let password = "every block pw";
    let encrypted = encrypt(&compressed, password);
    s.save(&encrypted);
    let read_back = s.read().expect("notes in the .tmp");
    assert_eq!(read_back, encrypted);

    let decrypted = crypto::decrypt(&read_back, password).unwrap();
    assert_eq!(decrypted.as_str(), compressed);
    let text = crate::features::compression::decompress(&decrypted);
    assert_eq!(text, combined);
    let (back_settings, back_note) = Settings::parse(&text);
    assert!(back_settings == settings, "settings read back");
    assert_eq!(back_note, note);
    let cats = crate::features::vault::categories::effective(&back_settings.vault_categories);
    let parsed = crate::features::payload::parse_payload(&back_note, &cats);
    let view: Vec<(String, String, Option<String>)> =
        parsed.tabs.iter().map(|t| (t.name.clone(), t.content.clone(), t.color.clone())).collect();
    let want: Vec<(String, String, Option<String>)> =
        tabs.iter().map(|t| (t.name.clone(), t.content.clone(), t.color.clone())).collect();
    assert_eq!(view, want);
    assert_eq!(parsed.prompts, prompts);
    assert_eq!(parsed.secrets, secrets);
    assert_eq!(parsed.attachments, atts);
    assert_eq!(parsed.session, Some(state));
}
