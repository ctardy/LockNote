//! Integration tests — full encrypt/decrypt/storage chain
//!
//! These tests exercise the interaction between crypto, storage, and settings
//! modules without depending on NWG or any UI code.

use crate::crypto;
use crate::settings::Settings;
use crate::storage;
use std::fs;

#[test]
fn full_chain_encrypt_settings_decrypt() {
    // Create settings + note, serialize, encrypt, decrypt, parse back
    let settings = Settings::default_public();
    let note = "My secret note with special chars: <>&\"' and unicode: café 🔒";
    let combined = settings.serialize(note);

    let password = "test_password_123";
    let encrypted = crypto::encrypt(&combined, password).unwrap();
    let decrypted = crypto::decrypt(&encrypted, password).unwrap();

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
    let encrypted1 = crypto::encrypt(plaintext, pw1).unwrap();
    assert!(crypto::decrypt(&encrypted1, pw1).is_ok());

    // Decrypt with pw1, re-encrypt with pw2
    let decrypted = crypto::decrypt(&encrypted1, pw1).unwrap();
    let encrypted2 = crypto::encrypt(&decrypted, pw2).unwrap();

    // pw1 no longer works on new ciphertext
    assert!(crypto::decrypt(&encrypted2, pw1).is_err());
    // pw2 works
    assert_eq!(crypto::decrypt(&encrypted2, pw2).unwrap(), plaintext);
}

#[test]
fn full_chain_storage_write_read_decrypt() {
    let dir = std::env::temp_dir().join("locknote_integration_test");
    let _ = fs::create_dir_all(&dir);
    let exe_file = dir.join("integration_test.exe");

    // Create fake exe
    let exe_stub = vec![0x4Du8, 0x5A, 0x90, 0x00];
    fs::write(&exe_file, &exe_stub).unwrap();

    // Encrypt and write
    let password = "integration_test_pw";
    let note = "Integration test note";
    let settings = Settings::default_public();
    let combined = settings.serialize(note);
    let encrypted = crypto::encrypt(&combined, password).unwrap();

    storage::write_data(&exe_file, &encrypted).unwrap();

    // Read back and decrypt
    let read_back = storage::read_data(&exe_file).unwrap();
    assert_eq!(read_back, encrypted);

    let decrypted = crypto::decrypt(&read_back, password).unwrap();
    let (_, parsed_note) = Settings::parse(&decrypted);
    assert_eq!(parsed_note, note);

    // Cleanup
    let tmp_path = storage::get_tmp_path(&exe_file);
    let _ = fs::remove_file(&tmp_path);
    let _ = fs::remove_file(&exe_file);
    let _ = fs::remove_dir(&dir);
}

#[test]
fn full_chain_corrupted_storage_fails_gracefully() {
    let dir = std::env::temp_dir().join("locknote_integration_corrupt");
    let _ = fs::create_dir_all(&dir);
    let exe_file = dir.join("corrupt_test.exe");

    let exe_stub = vec![0x4Du8, 0x5A];
    fs::write(&exe_file, &exe_stub).unwrap();

    let encrypted = crypto::encrypt("test", "pass").unwrap();
    storage::write_data(&exe_file, &encrypted).unwrap();

    // Corrupt the tmp file
    let tmp_path = storage::get_tmp_path(&exe_file);
    let mut data = fs::read(&tmp_path).unwrap();
    // Flip bytes in the encrypted payload area (near the end)
    let len = data.len();
    if len > 10 {
        data[len - 5] ^= 0xFF;
    }
    fs::write(&tmp_path, &data).unwrap();

    // Read back — should get corrupted data
    let read_back = storage::read_data(&exe_file).unwrap();
    // Decrypt should fail (HMAC mismatch)
    assert!(crypto::decrypt(&read_back, "pass").is_err());

    // Cleanup
    let _ = fs::remove_file(&tmp_path);
    let _ = fs::remove_file(&exe_file);
    let _ = fs::remove_dir(&dir);
}

#[test]
fn full_chain_empty_note() {
    let settings = Settings::default_public();
    let combined = settings.serialize("");
    let encrypted = crypto::encrypt(&combined, "pw").unwrap();
    let decrypted = crypto::decrypt(&encrypted, "pw").unwrap();
    let (_, note) = Settings::parse(&decrypted);
    assert_eq!(note, "");
}

#[test]
fn full_chain_large_note() {
    let large_note = "x".repeat(500_000);
    let settings = Settings::default_public();
    let combined = settings.serialize(&large_note);
    let encrypted = crypto::encrypt(&combined, "pw").unwrap();
    let decrypted = crypto::decrypt(&encrypted, "pw").unwrap();
    let (_, note) = Settings::parse(&decrypted);
    assert_eq!(note, large_note);
}

#[test]
fn storage_marker_preserved_across_updates() {
    // Simulates an exe update: new exe binary + old payload
    let dir = std::env::temp_dir().join("locknote_integration_update");
    let _ = fs::create_dir_all(&dir);
    let exe_file = dir.join("update_test.exe");

    // Create initial exe with data
    let exe_stub_v1 = vec![0xAAu8; 100];
    fs::write(&exe_file, &exe_stub_v1).unwrap();
    let payload = crypto::encrypt("original note", "pw").unwrap();
    storage::write_data(&exe_file, &payload).unwrap();

    // Simulate update: read the payload first
    let old_payload = storage::read_data(&exe_file).unwrap();

    // Create "new" exe binary (different size)
    let exe_stub_v2 = vec![0xBBu8; 150];
    fs::write(&exe_file, &exe_stub_v2).unwrap();

    // Write old payload to new exe
    storage::write_data(&exe_file, &old_payload).unwrap();

    // Verify data is still accessible
    let recovered = storage::read_data(&exe_file).unwrap();
    let decrypted = crypto::decrypt(&recovered, "pw").unwrap();
    assert_eq!(decrypted, "original note");

    // Cleanup
    let tmp_path = storage::get_tmp_path(&exe_file);
    let _ = fs::remove_file(&tmp_path);
    let _ = fs::remove_file(&exe_file);
    let _ = fs::remove_dir(&dir);
}
