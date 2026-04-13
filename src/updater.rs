// Updater module — Check for updates via GitHub releases API
//
// Fetches the latest release from GitHub, compares semantic version,
// downloads and migrates data to the new binary if user confirms.

use std::path::Path;

const CURRENT_VERSION: &str = "1.1.0";
const GITHUB_REPO: &str = "ctardy/LockNote";

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

/// GitHub release API URL.
fn api_url() -> String {
    format!("https://api.github.com/repos/{}/releases/latest", GITHUB_REPO)
}

/// Check for updates (blocking HTTP call).
pub fn check_for_update() -> UpdateCheckResult {
    let url = api_url();
    let response = match ureq::get(&url)
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

/// Download a zip file and extract LockNote.exe, migrate data to the new binary.
pub fn download_and_update(download_url: &str, exe_path: &Path) -> Result<String, String> {
    // Download zip to temp
    let response = ureq::get(download_url)
        .header("User-Agent", &format!("LockNote/{}", CURRENT_VERSION))
        .call()
        .map_err(|e| format!("Download failed: {}", e))?;

    let zip_data = response
        .into_body()
        .read_to_vec()
        .map_err(|e| format!("Read failed: {}", e))?;

    // Extract using zip crate (or fallback to temp file approach)
    let cursor = std::io::Cursor::new(&zip_data);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| format!("ZIP error: {}", e))?;

    // Find LockNote.exe in the archive
    let mut new_exe_data: Option<Vec<u8>> = None;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)
            .map_err(|e| format!("ZIP entry error: {}", e))?;
        let name = file.name().to_lowercase();
        if name.ends_with("locknote.exe") {
            let mut data = Vec::new();
            std::io::Read::read_to_end(&mut file, &mut data)
                .map_err(|e| format!("Extract error: {}", e))?;
            new_exe_data = Some(data);
            break;
        }
    }

    let new_exe = new_exe_data.ok_or("LockNote.exe not found in archive")?;

    // Read current encrypted payload
    let current_payload = crate::storage::read_data(exe_path);

    // Write new exe + old payload to staging .tmp
    let marker = crate::storage::get_marker_for_update();
    let tmp_path = crate::storage::get_tmp_path(exe_path);

    if let Some(parent) = tmp_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Dir error: {}", e))?;
    }

    let mut output = Vec::with_capacity(new_exe.len() + marker.len() + 1024);
    output.extend_from_slice(&new_exe);

    if let Some(payload) = current_payload {
        output.extend_from_slice(&marker);
        output.extend_from_slice(&payload);
    }

    std::fs::write(&tmp_path, &output)
        .map_err(|e| format!("Write error: {}", e))?;

    Ok("Update downloaded. Restart LockNote to apply.".into())
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
        let v = SemVer::current();
        assert_eq!(v, SemVer { major: 1, minor: 1, patch: 0 });
    }

    #[test]
    fn parse_empty_string() {
        assert_eq!(SemVer::parse(""), None);
    }

    #[test]
    fn parse_only_v_prefix() {
        assert_eq!(SemVer::parse("v"), None);
    }

    #[test]
    fn parse_two_parts() {
        assert_eq!(SemVer::parse("1.0"), None);
    }

    #[test]
    fn parse_four_parts() {
        assert_eq!(SemVer::parse("1.0.0.0"), None);
    }

    #[test]
    fn parse_with_leading_trailing_whitespace() {
        let v = SemVer::parse("  1.2.3  ");
        assert!(v.is_some());
        let v = v.unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
    }

    #[test]
    fn parse_negative_numbers() {
        assert_eq!(SemVer::parse("-1.0.0"), None);
    }

    #[test]
    fn parse_very_large_numbers() {
        let v = SemVer::parse("999999999.0.0");
        assert!(v.is_some());
        let v = v.unwrap();
        assert_eq!(v.major, 999999999);
    }

    #[test]
    fn parse_overflow() {
        assert_eq!(SemVer::parse("99999999999.0.0"), None);
    }

    #[test]
    fn parse_non_numeric() {
        assert_eq!(SemVer::parse("a.b.c"), None);
    }

    #[test]
    fn parse_mixed_valid_invalid() {
        assert_eq!(SemVer::parse("1.0.abc"), None);
    }

    #[test]
    fn version_equality_not_newer() {
        let v = SemVer::parse("1.0.0").unwrap();
        assert!(!v.is_newer_than(&v));
    }

    #[test]
    fn version_comparison_patch_only() {
        let a = SemVer::parse("1.0.2").unwrap();
        let b = SemVer::parse("1.0.1").unwrap();
        assert!(a.is_newer_than(&b));
    }

    #[test]
    fn version_comparison_minor_beats_patch() {
        let a = SemVer::parse("1.1.0").unwrap();
        let b = SemVer::parse("1.0.99").unwrap();
        assert!(a.is_newer_than(&b));
    }

    #[test]
    fn version_comparison_major_beats_all() {
        let a = SemVer::parse("2.0.0").unwrap();
        let b = SemVer::parse("1.99.99").unwrap();
        assert!(a.is_newer_than(&b));
    }

    #[test]
    fn version_display() {
        let v = SemVer { major: 1, minor: 2, patch: 3 };
        assert_eq!(format!("{}", v), "1.2.3");
    }

    #[test]
    fn version_display_zeros() {
        let v = SemVer { major: 0, minor: 0, patch: 0 };
        assert_eq!(format!("{}", v), "0.0.0");
    }

    #[test]
    fn current_version_matches_cargo() {
        assert_eq!(current_version(), "1.1.0");
    }

    #[test]
    fn github_repo_format() {
        assert!(github_repo().contains('/'));
    }

    #[test]
    fn parse_with_v_prefix_and_spaces() {
        let v = SemVer::parse("  v1.0.0  ");
        assert!(v.is_some());
        let v = v.unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 0);
        assert_eq!(v.patch, 0);
    }

    #[test]
    fn is_newer_symmetric() {
        let a = SemVer::parse("2.0.0").unwrap();
        let b = SemVer::parse("1.0.0").unwrap();
        assert!(a.is_newer_than(&b));
        assert!(!b.is_newer_than(&a));
    }
}
