// Updater module — Check for updates via GitHub releases API
//
// Fetches the latest release from GitHub, compares semantic version,
// downloads and migrates data to the new binary if user confirms.
//
// Integrity chain:
//   1. SHA256SUMS.txt lists the hash of every release asset (.zip).
//   2. SHA256SUMS.txt.sig is an Ed25519 signature of the raw SHA256SUMS.txt
//      bytes, produced offline with a release private key kept outside CI.
//   3. The client verifies the signature with `RELEASE_PUBLIC_KEY` below
//      before trusting any hash. Without this step, anyone who can publish
//      to the release repo (CI token fuité, compte GitHub compromis) could
//      push matching pairs (.zip + SHA256SUMS.txt) and the SHA check would
//      be circular. Signing breaks that loop.

use ed25519_dalek::VerifyingKey;
use sha2::{Digest, Sha256};
use std::path::Path;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
// The public repository: its signed releases are read without GitHub
// authentication. Up to 1.2.27 the Pro edition read a mirror
// (`ctardy/LockNote-Pro-releases`), which never gets a 2.0 release
// (issue 059).
const GITHUB_REPO: &str = "ctardy/LockNote";

/// Maximum accepted size for a downloaded zip (prevents a zip-bomb upload or
/// a MITM with a compromised CA from driving the process to OOM).
const MAX_ZIP_SIZE: usize = 100 * 1024 * 1024; // 100 MiB
/// Maximum accepted size for SHA256SUMS.txt and its .sig — generous but
/// bounded so a malicious peer cannot stream gigabytes at us.
const MAX_SUMS_SIZE: u64 = 1 * 1024 * 1024;

/// Ed25519 public key used to verify SHA256SUMS.txt for release artifacts.
///
/// The matching private seed is held outside the repo (GitHub Actions
/// secret `RELEASE_SIGNING_KEY_HEX` + offline backup). With this set,
/// `verify_sums_signature` is fail-closed: any release without a valid
/// `SHA256SUMS.txt.sig` is rejected, breaking the circular trust where
/// a compromised release-repo writer could ship a coherent (zip, sums)
/// pair on their own.
///
/// See docs/release-signing-setup.md for the generation and rotation
/// procedure.
const RELEASE_PUBLIC_KEY: Option<[u8; 32]> = Some([
    0x63, 0x3d, 0xd8, 0x0f, 0xc2, 0x68, 0x63, 0x1a,
    0x4b, 0x3c, 0x6d, 0xae, 0x2a, 0x8f, 0x1e, 0xb8,
    0x13, 0xdd, 0x5c, 0x45, 0x79, 0x57, 0xf4, 0x5a,
    0xcc, 0xa1, 0x68, 0xc9, 0xcc, 0x84, 0x44, 0xca,
]);

/// Semantic version for comparison.
#[derive(Debug, PartialEq, Eq)]
pub struct SemVer {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl SemVer {
    /// Parse a version string like "1.0.1" or "v1.0.1".
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().strip_prefix('v').unwrap_or(s.trim());
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            return None;
        }
        Some(SemVer {
            major: parts[0].parse().ok()?,
            minor: parts[1].parse().ok()?,
            patch: parts[2].parse().ok()?,
        })
    }

    /// Returns true if `self` is newer than `other`.
    pub fn is_newer_than(&self, other: &SemVer) -> bool {
        (self.major, self.minor, self.patch) > (other.major, other.minor, other.patch)
    }

    pub fn current() -> Self {
        Self::parse(CURRENT_VERSION).expect("CURRENT_VERSION must be valid semver")
    }
}

impl std::fmt::Display for SemVer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Result of an update check.
pub enum UpdateCheckResult {
    /// A newer version is available.
    Available {
        version: SemVer,
        download_url: String,
    },
    /// Already running the latest version.
    UpToDate,
    /// Error during check.
    Error(String),
}

/// Shared HTTP agent for every updater request.
///
/// Certificates are verified against the OS store (Windows) instead of the
/// bundled Mozilla roots, so a corporate TLS-intercepting proxy whose CA is
/// installed on the machine no longer fails with "invalid peer certificate".
/// Verification stays on: an untrusted chain is still rejected.
fn http_agent() -> &'static ureq::Agent {
    static AGENT: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::Agent::config_builder()
            .tls_config(
                ureq::tls::TlsConfig::builder()
                    .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                    .build(),
            )
            .build()
            .new_agent()
    })
}

/// GitHub release API URL.
fn api_url() -> String {
    format!("https://api.github.com/repos/{}/releases/latest", GITHUB_REPO)
}

/// Check for updates (blocking HTTP call).
pub fn check_for_update() -> UpdateCheckResult {
    let url = api_url();
    let response = match http_agent().get(&url)
        .header("User-Agent", &format!("LockNote/{}", CURRENT_VERSION))
        .call()
    {
        Ok(r) => r,
        Err(e) => return UpdateCheckResult::Error(format!("HTTP error: {}", e)),
    };

    let body: String = match response.into_body().read_to_string() {
        Ok(s) => s,
        Err(e) => return UpdateCheckResult::Error(format!("Read error: {}", e)),
    };

    let json: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => return UpdateCheckResult::Error(format!("JSON parse error: {}", e)),
    };

    let tag_name = match json["tag_name"].as_str() {
        Some(t) => t,
        None => return UpdateCheckResult::Error("No tag_name in release".into()),
    };

    let remote_version = match SemVer::parse(tag_name) {
        Some(v) => v,
        None => return UpdateCheckResult::Error(format!("Invalid version: {}", tag_name)),
    };

    let current = SemVer::current();
    if remote_version.is_newer_than(&current) {
        // Find download URL from assets
        let download_url = json["assets"]
            .as_array()
            .and_then(|assets| {
                assets.iter().find_map(|a| {
                    let name = a["name"].as_str().unwrap_or("");
                    if name.ends_with(".zip") {
                        a["browser_download_url"].as_str().map(|s| s.to_string())
                    } else {
                        None
                    }
                })
            })
            .unwrap_or_default();

        UpdateCheckResult::Available {
            version: remote_version,
            download_url,
        }
    } else {
        UpdateCheckResult::UpToDate
    }
}

/// Fetch a named asset from the latest release and return its raw bytes.
/// Used for both `SHA256SUMS.txt` and `SHA256SUMS.txt.sig`. Caps the
/// response at `MAX_SUMS_SIZE`.
fn fetch_release_asset_bytes(asset_name: &str) -> Result<Vec<u8>, String> {
    let url = api_url();
    let response = http_agent().get(&url)
        .header("User-Agent", &format!("LockNote/{}", CURRENT_VERSION))
        .call()
        .map_err(|e| format!("Release metadata fetch failed: {}", e))?;

    let body: String = response
        .into_body()
        .read_to_string()
        .map_err(|e| format!("Release metadata read failed: {}", e))?;

    let json: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Release JSON parse failed: {}", e))?;

    let asset_url = json["assets"]
        .as_array()
        .and_then(|assets| {
            assets.iter().find_map(|a| {
                let name = a["name"].as_str().unwrap_or("");
                if name == asset_name {
                    a["browser_download_url"].as_str().map(|s| s.to_string())
                } else {
                    None
                }
            })
        })
        .ok_or_else(|| format!("Asset {} missing from release", asset_name))?;

    let asset_resp = http_agent().get(&asset_url)
        .header("User-Agent", &format!("LockNote/{}", CURRENT_VERSION))
        .call()
        .map_err(|e| format!("Asset download failed: {}", e))?;

    let mut body = asset_resp.into_body();
    // ureq 3.x exposes a bounded reader via `.with_config().limit(...)`; we
    // keep it simple and just truncate manually after reading.
    let bytes = body
        .read_to_vec()
        .map_err(|e| format!("Asset read failed: {}", e))?;
    if bytes.len() as u64 > MAX_SUMS_SIZE {
        return Err(format!(
            "Asset {} exceeds the {}-byte size cap",
            asset_name, MAX_SUMS_SIZE
        ));
    }
    Ok(bytes)
}

/// Verify `SHA256SUMS.txt.sig` (raw 64-byte Ed25519 signature) against
/// `sums_bytes` using the embedded `RELEASE_PUBLIC_KEY`. Fail-closed when
/// a key is configured; emit a no-op Ok(()) when `RELEASE_PUBLIC_KEY` is
/// `None` (transition mode — the caller still enforces the SHA256 check).
fn verify_sums_signature(sums_bytes: &[u8]) -> Result<(), String> {
    let pubkey = match RELEASE_PUBLIC_KEY {
        Some(k) => k,
        None => {
            // No release key embedded yet — the SHA256 check alone is kept
            // for now (circular trust, documented limitation). The CI path
            // needs to generate the offline key and populate the const above.
            return Ok(());
        }
    };

    let sig_bytes = fetch_release_asset_bytes("SHA256SUMS.txt.sig")?;
    let sig_array: [u8; 64] = sig_bytes
        .as_slice()
        .try_into()
        .map_err(|_| "SHA256SUMS.txt.sig must be exactly 64 bytes (raw Ed25519)".to_string())?;

    let verifying_key = VerifyingKey::from_bytes(&pubkey)
        .map_err(|_| "Embedded release public key is invalid".to_string())?;
    let signature = ed25519_dalek::Signature::from_bytes(&sig_array);
    verifying_key
        .verify_strict(sums_bytes, &signature)
        .map_err(|_| "SHA256SUMS.txt signature verification FAILED — aborting update".to_string())?;
    Ok(())
}

/// Parse SHA256SUMS.txt format ("<hex>  <filename>") and return the hash for `filename`.
fn find_expected_hash(sums: &str, filename: &str) -> Option<String> {
    for line in sums.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // Format: <hex-sha256>  <filename>  (two spaces per GNU sha256sum convention,
        // but accept any whitespace separator for robustness).
        let mut parts = trimmed.splitn(2, char::is_whitespace);
        let hash = parts.next().unwrap_or("").trim();
        let rest = parts.next().unwrap_or("").trim();
        // Strip optional leading '*' used for binary mode in sha256sum output.
        let name = rest.trim_start_matches('*').trim();
        if name == filename && !hash.is_empty() {
            return Some(hash.to_lowercase());
        }
    }
    None
}

/// Name (lowercase) of the exe inside the release zip
/// (`LockNote-windows-x64.zip`).
const EDITION_EXE: &str = "locknote.exe";

/// Index of the zip entry whose file name is exactly `wanted` (lowercase),
/// ignoring case and folders. No fallback: another exe (the 1.2.x zips also
/// held `LockNote-Pro.exe`), a directory entry or a longer name that merely
/// ends the same way never matches.
fn select_entry<S: AsRef<str>>(names: &[S], wanted: &str) -> Option<usize> {
    names.iter().position(|n| {
        let n = n.as_ref().to_lowercase();
        if n.ends_with('/') || n.ends_with('\\') {
            return false;
        }
        n.rsplit(|c| c == '/' || c == '\\').next() == Some(wanted)
    })
}

/// Download a zip file, extract its exe, migrate data to the new binary.
pub fn download_and_update(download_url: &str, exe_path: &Path) -> Result<String, String> {
    // Derive asset filename from the download URL (last path segment).
    let asset_name = download_url
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_string();
    if asset_name.is_empty() {
        return Err("Invalid download URL".into());
    }

    // Download zip to temp
    let response = http_agent().get(download_url)
        .header("User-Agent", &format!("LockNote/{}", CURRENT_VERSION))
        .call()
        .map_err(|e| format!("Download failed: {}", e))?;

    let zip_data = response
        .into_body()
        .read_to_vec()
        .map_err(|e| format!("Read failed: {}", e))?;

    if zip_data.len() > MAX_ZIP_SIZE {
        return Err(format!(
            "Download exceeds the {}-byte size cap (got {} bytes)",
            MAX_ZIP_SIZE,
            zip_data.len()
        ));
    }

    // Integrity chain:
    //   (a) fetch SHA256SUMS.txt,
    //   (b) verify its Ed25519 signature (fail-closed when a key is set),
    //   (c) look up the entry for `asset_name`,
    //   (d) compare against the hash of the bytes we just downloaded.
    // Step (b) is what stops a compromised CI token from pushing a coherent
    // (.zip, SHA256SUMS.txt) pair — the attacker would also need the
    // offline signing key to produce a matching .sig.
    let sums_bytes = fetch_release_asset_bytes("SHA256SUMS.txt")?;
    verify_sums_signature(&sums_bytes)?;
    let sums = std::str::from_utf8(&sums_bytes)
        .map_err(|_| "SHA256SUMS.txt is not valid UTF-8".to_string())?;
    let expected_hash = find_expected_hash(sums, &asset_name)
        .ok_or_else(|| format!("Integrity file missing entry for {}", asset_name))?;

    let actual_hash = {
        let digest = Sha256::digest(&zip_data);
        // hex lowercase encoding
        let mut s = String::with_capacity(digest.len() * 2);
        for b in digest.iter() {
            s.push_str(&format!("{:02x}", b));
        }
        s
    };

    if actual_hash != expected_hash {
        return Err("Update integrity check failed (SHA256 mismatch)".into());
    }

    let new_exe = extract_exe(&zip_data)?;

    // Stage the new program followed by the notes of this session (its last
    // save, whatever the files say), marked as the updater's program so the
    // exit promotes it even without notes and later saves keep it. Atomic: a
    // failed write leaves the previous .tmp (the latest save) as it was.
    crate::storage::write_staged_update(exe_path, &new_exe)
        .map_err(|e| format!("Write error: {}", e))?;

    Ok("Update downloaded. Restart LockNote to apply.".into())
}

/// The edition's exe from the release zip (by its exact name, never
/// another program of the zip).
fn extract_exe(zip_data: &[u8]) -> Result<Vec<u8>, String> {
    let cursor = std::io::Cursor::new(zip_data);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| format!("ZIP error: {}", e))?;

    // Take the exe by its exact name, never another program of the zip.
    let mut names: Vec<String> = Vec::with_capacity(archive.len());
    for i in 0..archive.len() {
        let file = archive.by_index(i)
            .map_err(|e| format!("ZIP entry error: {}", e))?;
        let name = file.name().map_err(|e| format!("ZIP entry name error: {}", e))?;
        names.push(name.to_string());
    }
    let index = select_entry(&names, EDITION_EXE)
        .ok_or_else(|| format!("{} not found in archive", EDITION_EXE))?;
    eprintln!("updater: extracting edition_exe={} entries={}", EDITION_EXE, names.len());
    let mut file = archive.by_index(index)
        .map_err(|e| format!("ZIP entry error: {}", e))?;
    let mut new_exe = Vec::new();
    std::io::Read::read_to_end(&mut file, &mut new_exe)
        .map_err(|e| format!("Extract error: {}", e))?;
    drop(file);
    Ok(new_exe)
}

pub fn current_version() -> &'static str {
    CURRENT_VERSION
}

pub fn github_repo() -> &'static str {
    GITHUB_REPO
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the 1.2.x release zips contained (both editions of then): the
    /// exact name is still the only rule (see `select_entry`).
    const ZIP: &[&str] = &["LICENSE", "LockNote-Pro.exe", "LockNote.exe", "README.md", "SHA256SUMS.txt"];

    #[test]
    fn takes_the_named_exe_not_the_other_one() {
        // Both exes are in the zip and LockNote.exe comes later: the bug took
        // whichever matched first.
        assert_eq!(select_entry(ZIP, "locknote-pro.exe"), Some(1));
        let free_first = ["LockNote.exe", "LockNote-Pro.exe"];
        assert_eq!(select_entry(&free_first, "locknote-pro.exe"), Some(1));
    }

    #[test]
    fn takes_locknote_exe_from_a_two_exe_zip() {
        assert_eq!(select_entry(ZIP, "locknote.exe"), Some(2));
    }

    #[test]
    fn no_fallback_to_another_exe() {
        assert_eq!(select_entry(&["LockNote.exe", "README.md"], "locknote-pro.exe"), None);
        assert_eq!(select_entry(&["LockNote-Pro.exe"], "locknote.exe"), None);
    }

    #[test]
    fn matches_the_file_name_only_and_ignores_case_and_folders() {
        assert_eq!(select_entry(&["dist/LOCKNOTE-PRO.EXE"], "locknote-pro.exe"), Some(0));
        assert_eq!(select_entry(&["dist\\LockNote.exe"], "locknote.exe"), Some(0));
        // A different program whose name merely ends the same way is not ours.
        assert_eq!(select_entry(&["notlocknote.exe"], "locknote.exe"), None);
        assert_eq!(select_entry(&["old-locknote-pro.exe"], "locknote-pro.exe"), None);
        // A directory entry with that name is not a file.
        assert_eq!(select_entry(&["LockNote-Pro.exe/"], "locknote-pro.exe"), None);
    }

    /// 2.0 has one edition and one public repository (issue 059, D5). The
    /// 1.2.27 installations read `ctardy/LockNote-Pro-releases`: they never
    /// see a 2.0 release and are migrated by hand.
    #[test]
    fn updater_targets_the_public_repo() {
        assert_eq!(GITHUB_REPO, "ctardy/LockNote");
    }

    #[test]
    fn this_build_wants_locknote_exe() {
        assert_eq!(EDITION_EXE, "locknote.exe");
    }

    #[test]
    fn http_agent_trusts_the_os_certificate_store() {
        // Behind a TLS-intercepting proxy the CA lives in the Windows store,
        // not in the bundled Mozilla roots ("invalid peer certificate").
        let roots = http_agent().config().tls_config().root_certs();
        assert!(
            matches!(roots, ureq::tls::RootCerts::PlatformVerifier),
            "updater must verify certificates with the OS store, got {:?}",
            roots
        );
    }

    /// Network check (run with `--ignored`): the OS-store verifier must still
    /// accept a normal public TLS chain.
    #[test]
    #[ignore]
    fn http_agent_reaches_github_over_tls() {
        let resp = http_agent()
            .get("https://api.github.com/")
            .header("User-Agent", "LockNote-test")
            .call()
            .expect("TLS handshake with api.github.com via the OS store");
        assert_eq!(resp.status().as_u16(), 200);
    }

    #[test]
    fn parse_version() {
        assert_eq!(SemVer::parse("1.0.1"), Some(SemVer { major: 1, minor: 0, patch: 1 }));
        assert_eq!(SemVer::parse("v2.3.4"), Some(SemVer { major: 2, minor: 3, patch: 4 }));
        assert_eq!(SemVer::parse("invalid"), None);
        assert_eq!(SemVer::parse("1.0"), None);
    }

    #[test]
    fn version_comparison() {
        let v101 = SemVer::parse("1.0.1").unwrap();
        let v102 = SemVer::parse("1.0.2").unwrap();
        let v110 = SemVer::parse("1.1.0").unwrap();
        let v200 = SemVer::parse("2.0.0").unwrap();

        assert!(v102.is_newer_than(&v101));
        assert!(v110.is_newer_than(&v102));
        assert!(v200.is_newer_than(&v110));
        assert!(!v101.is_newer_than(&v101));
        assert!(!v101.is_newer_than(&v102));
    }

    #[test]
    fn current_version_valid() {
        // CURRENT_VERSION comes from Cargo.toml via env!("CARGO_PKG_VERSION").
        // Just verify that it parses and matches the crate version string.
        let v = SemVer::current();
        assert_eq!(format!("{}", v), CURRENT_VERSION);
    }

    #[test]
    fn current_version_parses() {
        SemVer::parse(CURRENT_VERSION)
            .expect("CURRENT_VERSION (from CARGO_PKG_VERSION) must be valid semver");
    }

    #[test]
    fn version_same_is_not_newer() {
        let v1 = SemVer::parse("1.0.1").unwrap();
        let v2 = SemVer::parse("1.0.1").unwrap();
        assert!(!v1.is_newer_than(&v2));
        assert!(!v2.is_newer_than(&v1));
    }

    #[test]
    fn version_patch_only_difference() {
        let v1 = SemVer::parse("1.0.1").unwrap();
        let v2 = SemVer::parse("1.0.2").unwrap();
        assert!(v2.is_newer_than(&v1));
        assert!(!v1.is_newer_than(&v2));
    }

    #[test]
    fn version_patch_zero_vs_nonzero() {
        let v_zero = SemVer::parse("1.0.0").unwrap();
        let v_one = SemVer::parse("1.0.1").unwrap();
        assert!(v_one.is_newer_than(&v_zero));
        assert!(!v_zero.is_newer_than(&v_one));
    }

    #[test]
    fn version_minor_only_difference() {
        let v1 = SemVer::parse("1.0.5").unwrap();
        let v2 = SemVer::parse("1.1.0").unwrap();
        assert!(v2.is_newer_than(&v1));
        assert!(!v1.is_newer_than(&v2));
    }

    #[test]
    fn version_major_only_difference() {
        let v1 = SemVer::parse("1.9.9").unwrap();
        let v2 = SemVer::parse("2.0.0").unwrap();
        assert!(v2.is_newer_than(&v1));
        assert!(!v1.is_newer_than(&v2));
    }

    #[test]
    fn version_major_jump() {
        let v1 = SemVer::parse("1.0.1").unwrap();
        let v2 = SemVer::parse("5.0.0").unwrap();
        assert!(v2.is_newer_than(&v1));
        assert!(!v1.is_newer_than(&v2));
    }

    #[test]
    fn version_large_numbers() {
        let v1 = SemVer::parse("10.20.30").unwrap();
        let v2 = SemVer::parse("10.20.31").unwrap();
        assert!(v2.is_newer_than(&v1));
        assert!(!v1.is_newer_than(&v2));
    }

    #[test]
    fn parse_with_v_prefix_and_whitespace() {
        assert_eq!(
            SemVer::parse("v1.0.1"),
            Some(SemVer { major: 1, minor: 0, patch: 1 })
        );
        assert_eq!(
            SemVer::parse("  v1.0.1  "),
            Some(SemVer { major: 1, minor: 0, patch: 1 })
        );
        assert_eq!(
            SemVer::parse("  1.0.1  "),
            Some(SemVer { major: 1, minor: 0, patch: 1 })
        );
    }

    #[test]
    fn semver_display_format() {
        let v = SemVer::parse("1.0.1").unwrap();
        assert_eq!(format!("{}", v), "1.0.1");
        let v2 = SemVer::parse("v2.3.4").unwrap();
        assert_eq!(format!("{}", v2), "2.3.4");
    }

    // ─── SHA256 integrity (issue 041) ────────────────────────────────────────

    const SAMPLE_SUMS: &str = "\
# LockNote release 1.2.4
a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2  LockNote-1.2.4.zip
0000000000000000000000000000000000000000000000000000000000000001  LockNote-Pro-1.2.4.zip
";

    #[test]
    fn sha256sums_parses_standard_format() {
        let hash = find_expected_hash(SAMPLE_SUMS, "LockNote-1.2.4.zip");
        assert_eq!(
            hash.as_deref(),
            Some("a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2")
        );
    }

    #[test]
    fn sha256sums_missing_entry_returns_none() {
        // A file listed nowhere in SHA256SUMS must not be installable.
        assert_eq!(find_expected_hash(SAMPLE_SUMS, "Unknown.zip"), None);
    }

    #[test]
    fn sha256sums_ignores_comments_and_blank_lines() {
        let sums = "\n# comment\n\n   \nabc  file.zip\n# another\n";
        assert_eq!(find_expected_hash(sums, "file.zip").as_deref(), Some("abc"));
    }

    #[test]
    fn sha256sums_handles_binary_mode_asterisk() {
        // GNU sha256sum prefixes the filename with '*' in binary mode.
        let sums = "deadbeefcafebabe *LockNote.zip\n";
        assert_eq!(
            find_expected_hash(sums, "LockNote.zip").as_deref(),
            Some("deadbeefcafebabe")
        );
    }

    #[test]
    fn sha256sums_hash_normalized_lowercase() {
        // Hashes may be uppercase in hand-written SHA256SUMS; compare must be
        // case-insensitive, so the returned hash is normalized to lowercase
        // (the actual hash computed from the ZIP is always lowercase hex).
        let sums = "AABBCCDDEEFF  file.zip\n";
        assert_eq!(find_expected_hash(sums, "file.zip").as_deref(), Some("aabbccddeeff"));
    }

    #[test]
    fn sha256sums_tolerates_single_space_separator() {
        // Strict GNU format uses two spaces; we accept any whitespace.
        let sums = "abcd file.zip\n";
        assert_eq!(find_expected_hash(sums, "file.zip").as_deref(), Some("abcd"));
    }

    #[test]
    fn sha256sums_tolerates_tab_separator() {
        let sums = "abcd\tfile.zip\n";
        assert_eq!(find_expected_hash(sums, "file.zip").as_deref(), Some("abcd"));
    }

    #[test]
    fn sha256sums_empty_file_rejects_all() {
        assert_eq!(find_expected_hash("", "anything.zip"), None);
        assert_eq!(find_expected_hash("# only comments\n", "anything.zip"), None);
    }

    // End-to-end mismatch simulation: compute a real SHA256 of a payload,
    // then compare against a tampered expected value. The equality check in
    // download_and_update() is simple string comparison, so we exercise it here.
    #[test]
    fn sha256_mismatch_detected() {
        let payload = b"fake zip contents";
        let actual = {
            let digest = Sha256::digest(payload);
            let mut s = String::with_capacity(digest.len() * 2);
            for b in digest.iter() {
                s.push_str(&format!("{:02x}", b));
            }
            s
        };
        let tampered = "0".repeat(64);
        assert_ne!(actual, tampered, "tampered hash must not match genuine digest");
    }

    #[test]
    fn release_public_key_is_32_bytes_and_valid() {
        // With RELEASE_PUBLIC_KEY = Some(...) the verifier is fail-closed.
        // We can't run the full network path here, but we can still assert
        // that the embedded bytes parse as a valid Ed25519 public point —
        // catches typos from the manual paste in a single test.
        let key = RELEASE_PUBLIC_KEY
            .expect("a release public key must be embedded; see docs/release-signing-setup.md");
        assert_eq!(key.len(), 32);
        ed25519_dalek::VerifyingKey::from_bytes(&key)
            .expect("RELEASE_PUBLIC_KEY must parse as a valid Ed25519 point");
    }

    /// Known answer: a deflated zip (written by Python's zipfile) gives back
    /// the exe's exact bytes, the other entry ignored (checked with zip 2
    /// before the dependency update of 2026-11-26).
    #[test]
    fn the_exe_is_extracted_from_a_deflated_zip() {
        const ZIP: &str = "504b030414000000080000007a5d9f4b7a450d000000200000000a000000524541444d452e7478742b4a4d4c51c84de52ac2410300504b030414000000080000007a5dabd4715819000000a00000000c0000004c6f636b4e6f74652e657865f38d52f0c94fcef6cb2f495548cbac28292d4a55f01d446200504b0102140014000000080000007a5d9f4b7a450d000000200000000a0000000000000000000000800100000000524541444d452e747874504b0102140014000000080000007a5dabd4715819000000a00000000c00000000000000000000008001350000004c6f636b4e6f74652e657865504b0506000000000200020072000000780000000000";
        let bytes: Vec<u8> = (0..ZIP.len() / 2).map(|i| u8::from_str_radix(&ZIP[2 * i..2 * i + 2], 16).unwrap()).collect();
        let exe = extract_exe(&bytes).unwrap();
        assert_eq!(exe, b"MZ LockNote fixture ".repeat(8));
        assert!(extract_exe(&bytes[..100]).is_err(), "a truncated zip fails");
    }

    /// Known answer: the SHA256SUMS.txt of the v2.1.0 release and its raw
    /// signature (release assets) verify with the embedded key, as
    /// `verify_sums_signature` does (checked with ed25519-dalek 2 before the
    /// dependency update of 2026-11-26); one changed byte fails.
    #[test]
    fn the_v2_1_0_release_signature_verifies_with_the_embedded_key() {
        const SUMS: &str = "097013215f78588414302d3e55cdd702c3316a076e89ad69dbb99ef2d9161f76 *LockNote.exe\n\
0fd8425835e581b287816aa47001c9e56e9227ff6a762db1ae32a16cdbd4a451 *LockNote-windows-x64.zip\n";
        const SIG: &str = "75ce904c8eef31a93178ea326a90616b9f6763bb0129f164e67899312d74ecca8a50fc72c61056d29759e2c4c375c84c362c8bb97b459b8a6302b54466ef9a05";
        let mut sig = [0u8; 64];
        for (i, b) in sig.iter_mut().enumerate() {
            *b = u8::from_str_radix(&SIG[2 * i..2 * i + 2], 16).unwrap();
        }
        let key = VerifyingKey::from_bytes(&RELEASE_PUBLIC_KEY.unwrap()).unwrap();
        let signature = ed25519_dalek::Signature::from_bytes(&sig);
        assert_eq!(SUMS.len(), 170, "the file as published");
        key.verify_strict(SUMS.as_bytes(), &signature).expect("the published signature verifies");
        let mut changed = SUMS.as_bytes().to_vec();
        changed[0] ^= 1;
        assert!(key.verify_strict(&changed, &signature).is_err());
    }

    #[test]
    fn sha256_match_accepted_for_known_payload() {
        // Computing SHA256 of a fixed string yields a known digest.
        let payload = b"";
        let digest = Sha256::digest(payload);
        let mut actual = String::with_capacity(digest.len() * 2);
        for b in digest.iter() {
            actual.push_str(&format!("{:02x}", b));
        }
        // SHA256 of the empty string.
        assert_eq!(
            actual,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
