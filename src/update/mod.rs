//! In-app auto-update: check GitHub Releases, and (on Windows) apply.
use serde::Deserialize;

#[cfg(windows)]
pub mod apply;

/// The release-asset platform token for this build. Matches the CI matrix `name`.
/// Windows-only today; a future build adds other-OS arms.
pub const PLATFORM: &str = "windows-x64";

/// The release asset file name for a given bare version (no leading `v`).
pub fn asset_name(version: &str) -> String {
    format!("g13-driver-v{version}-{PLATFORM}.zip")
}

fn parse_ver(s: &str) -> Option<semver::Version> {
    semver::Version::parse(s.trim().trim_start_matches('v')).ok()
}

/// True iff `latest` is a strictly newer semver than `current` (either may have a `v`).
pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_ver(latest), parse_ver(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

#[derive(Debug, Deserialize)]
pub struct Asset {
    pub name: String,
    #[serde(rename = "browser_download_url")]
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct ReleaseInfo {
    pub tag_name: String,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

pub fn parse_release(json: &str) -> anyhow::Result<ReleaseInfo> {
    Ok(serde_json::from_str(json)?)
}

/// A newer release with this platform's downloadable assets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailableUpdate {
    pub version: String,
    pub zip_url: String,
    pub sha256_url: String,
}

/// If `release` is newer than `current` AND has this platform's zip + .sha256, return them.
pub fn select_update(release: &ReleaseInfo, current: &str) -> Option<AvailableUpdate> {
    let latest = release.tag_name.trim_start_matches('v');
    if !is_newer(latest, current) {
        return None;
    }
    let zip = asset_name(latest);
    let sha = format!("{zip}.sha256");
    let zip_url = release.assets.iter().find(|a| a.name == zip)?.url.clone();
    let sha256_url = release.assets.iter().find(|a| a.name == sha)?.url.clone();
    Some(AvailableUpdate { version: latest.to_string(), zip_url, sha256_url })
}

/// GUI-facing update state (read by the Settings tab).
#[derive(Debug, Clone)]
pub enum UpdateStatus {
    Idle,
    Checking,
    UpToDate,
    Available(AvailableUpdate),
    Installing,
    Failed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_name_format() {
        assert_eq!(asset_name("0.2.0"), "g13-driver-v0.2.0-windows-x64.zip");
    }

    #[test]
    fn is_newer_compares_semver() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("v0.2.0", "0.1.0"));      // leading v tolerated
        assert!(!is_newer("0.1.0", "0.1.0"));      // equal is not newer
        assert!(!is_newer("0.1.0", "0.2.0"));      // older
        assert!(is_newer("1.0.0", "1.0.0-rc.1"));  // release > prerelease
        assert!(!is_newer("garbage", "0.1.0"));    // unparseable -> not newer
    }

    #[test]
    fn select_update_picks_platform_asset_when_newer() {
        let json = r#"{
          "tag_name": "v0.2.0",
          "assets": [
            {"name": "g13-driver-v0.2.0-windows-x64.zip", "browser_download_url": "https://x/zip"},
            {"name": "g13-driver-v0.2.0-windows-x64.zip.sha256", "browser_download_url": "https://x/sha"},
            {"name": "some-other-file.txt", "browser_download_url": "https://x/other"}
          ]
        }"#;
        let rel = parse_release(json).unwrap();
        let u = select_update(&rel, "0.1.0").expect("update offered");
        assert_eq!(u.version, "0.2.0");
        assert_eq!(u.zip_url, "https://x/zip");
        assert_eq!(u.sha256_url, "https://x/sha");
    }

    #[test]
    fn select_update_none_when_not_newer() {
        let json = r#"{"tag_name":"v0.1.0","assets":[
          {"name":"g13-driver-v0.1.0-windows-x64.zip","browser_download_url":"u"},
          {"name":"g13-driver-v0.1.0-windows-x64.zip.sha256","browser_download_url":"u"}]}"#;
        let rel = parse_release(json).unwrap();
        assert!(select_update(&rel, "0.1.0").is_none());
    }

    #[test]
    fn select_update_none_when_platform_asset_missing() {
        let json = r#"{"tag_name":"v0.2.0","assets":[
          {"name":"g13-driver-v0.2.0-linux-x64.tar.gz","browser_download_url":"u"}]}"#;
        let rel = parse_release(json).unwrap();
        assert!(select_update(&rel, "0.1.0").is_none());
    }
}
