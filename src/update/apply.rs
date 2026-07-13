//! Windows apply step: download, verify, extract, self-replace, restart.
#![cfg(windows)]

use anyhow::{bail, Context, Result};
use std::io::{Read, Write};
use std::path::Path;
use crate::update::AvailableUpdate;

/// Lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// The hex digest from a `sha256sum`-format file (`<hex>  <name>`), lowercased.
pub fn parse_sha256_file(contents: &str) -> Option<String> {
    contents.split_whitespace().next().map(|s| s.to_lowercase())
}

fn download_to_file(url: &str, dest: &Path) -> Result<()> {
    let mut response = ureq::get(url)
        .header("User-Agent", concat!("g13-driver/", env!("G13_VERSION")))
        .call()?;
    let mut reader = response.body_mut().as_reader();
    let mut out = std::fs::File::create(dest)?;
    std::io::copy(&mut reader, &mut out)?;
    out.flush()?;
    Ok(())
}

fn download_to_string(url: &str) -> Result<String> {
    let mut response = ureq::get(url)
        .header("User-Agent", concat!("g13-driver/", env!("G13_VERSION")))
        .call()?;
    Ok(response.body_mut().read_to_string()?)
}

/// Extract just `g13-driver.exe` from the release zip's nested folder to `dest`.
/// Falls back to the single `*.exe` entry if the exact nested path isn't found.
fn extract_exe(zip_path: &Path, version: &str, dest: &Path) -> Result<()> {
    let file = std::fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file).context("open zip")?;
    let exact = format!("g13-driver-v{version}-{}/g13-driver.exe", crate::update::PLATFORM);
    // Find the exact nested path, else any entry ending in g13-driver.exe.
    let idx = (0..archive.len()).find(|&i| {
        archive.by_index(i).map(|e| e.name() == exact).unwrap_or(false)
    }).or_else(|| (0..archive.len()).find(|&i| {
        archive.by_index(i).map(|e| e.name().ends_with("g13-driver.exe")).unwrap_or(false)
    }));
    let idx = idx.context("g13-driver.exe not found in archive")?;
    let mut entry = archive.by_index(idx)?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf)?;
    std::fs::write(dest, &buf)?;
    Ok(())
}

fn swap_and_restart(_new_exe: &Path) -> Result<()> {
    bail!("swap_and_restart not implemented yet")
}

/// Download the update, verify its SHA-256, extract the new exe, then swap+restart.
pub fn install(update: &AvailableUpdate) -> Result<()> {
    let tmp = std::env::temp_dir();
    let zip_path = tmp.join(format!("g13-update-{}.zip", update.version));
    download_to_file(&update.zip_url, &zip_path).context("download zip")?;

    let sha_txt = download_to_string(&update.sha256_url).context("download sha256")?;
    let expected = parse_sha256_file(&sha_txt).context("empty/invalid sha256 file")?;
    let bytes = std::fs::read(&zip_path)?;
    let actual = sha256_hex(&bytes);
    if actual != expected {
        let _ = std::fs::remove_file(&zip_path);
        bail!("checksum mismatch (expected {expected}, got {actual}) — aborting update");
    }

    let new_exe = tmp.join(format!("g13-driver-new-{}.exe", update.version));
    extract_exe(&zip_path, &update.version, &new_exe).context("extract exe")?;
    let _ = std::fs::remove_file(&zip_path);

    swap_and_restart(&new_exe)  // implemented in Task 4
}

#[cfg(test)]
mod tests {
    use super::{parse_sha256_file, sha256_hex};

    #[test]
    fn sha256_of_known_bytes() {
        // SHA-256("abc")
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        // SHA-256("")
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn parse_sha256_takes_first_field_lowercased() {
        assert_eq!(
            parse_sha256_file("ABC123  g13-driver-v0.2.0-windows-x64.zip\n"),
            Some("abc123".to_string())
        );
        assert_eq!(parse_sha256_file(""), None);
    }
}
