use std::collections::HashSet;
use std::path::Path;
use anyhow::{Context, Result};
use serde::Deserialize;
use crate::config::{Profile, ProfileSource};

const REPO: &str = "cavefish-dev/g13-driver";
const BRANCH: &str = "main";

/// One entry in the catalog index.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CatalogEntry {
    pub filename: String,
    pub name: String,
}

/// GUI-facing catalog state (read by the Catalog tab).
#[derive(Debug, Clone)]
pub enum CatalogState {
    Idle,
    Loading,
    Loaded(Vec<CatalogEntry>),
    Failed(String),
}

pub fn index_url() -> String {
    format!("https://raw.githubusercontent.com/{REPO}/{BRANCH}/catalog/index.json")
}

pub fn profile_url(filename: &str) -> String {
    format!("https://raw.githubusercontent.com/{REPO}/{BRANCH}/catalog/{filename}")
}

pub fn parse_index(json: &str) -> Result<Vec<CatalogEntry>> {
    serde_json::from_str(json).context("catalog index is not valid JSON")
}

/// Pair each entry with whether a local profile already carries its filename as `origin`.
pub fn mark_downloaded(entries: Vec<CatalogEntry>, local_origins: &HashSet<String>) -> Vec<(CatalogEntry, bool)> {
    entries.into_iter()
        .map(|e| { let dl = local_origins.contains(&e.filename); (e, dl) })
        .collect()
}

/// Stamp a freshly-fetched profile as a GitHub download (keeps its upstream name).
pub fn stamp_download(profile: &mut Profile, origin: &str) {
    profile.set_source(ProfileSource::Github);
    profile.set_origin(Some(origin.to_string()));
    profile.set_modified(false);
}

/// Parse-validate downloaded bytes as a real profile (the integrity gate).
pub fn parse_profile(body: &str, filename: &str) -> Result<Profile> {
    let raw = toml::from_str(body)
        .with_context(|| format!("catalog profile {filename} is not valid TOML"))?;
    Profile::from_raw(raw)
        .with_context(|| format!("catalog profile {filename} is not a valid profile"))
}

fn user_agent() -> String {
    format!("g13-driver/{}", env!("G13_VERSION"))
}

fn http_get(url: &str) -> Result<String> {
    let body = ureq::get(url)
        .header("User-Agent", &user_agent())
        .call()?
        .body_mut()
        .read_to_string()?;
    Ok(body)
}

/// Fetch + parse the catalog index.
pub fn fetch_index() -> Result<Vec<CatalogEntry>> {
    let body = http_get(&index_url())?;
    parse_index(&body)
}

/// Fetch + validate a catalog profile by its catalog filename.
fn fetch_profile(filename: &str) -> Result<Profile> {
    let body = http_get(&profile_url(filename))?;
    parse_profile(&body, filename)
}

/// Download a catalog profile into `dir` (stamped), returning the new local filename.
pub fn download(dir: &Path, filename: &str) -> Result<String> {
    let mut profile = fetch_profile(filename)?;
    stamp_download(&mut profile, filename);
    let label = profile.meta_name().unwrap_or(filename.trim_end_matches(".toml")).to_string();
    let local = crate::profiles::unique_filename(dir, &label);
    std::fs::write(dir.join(&local), profile.to_toml()?)
        .with_context(|| format!("failed to write {local}"))?;
    Ok(local)
}

/// Revert the file at `path` to its upstream (`origin`) version, wholesale.
pub fn revert(path: &Path, origin: &str) -> Result<()> {
    let mut profile = fetch_profile(origin)?;
    stamp_download(&mut profile, origin);
    std::fs::write(path, profile.to_toml()?)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn parses_index() {
        let json = r#"[{"filename":"gaming.toml","name":"Gaming"},{"filename":"coding.toml","name":"Coding"}]"#;
        let v = parse_index(json).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].filename, "gaming.toml");
        assert_eq!(v[0].name, "Gaming");
    }

    #[test]
    fn parses_empty_index() {
        assert_eq!(parse_index("[]").unwrap().len(), 0);
    }

    #[test]
    fn malformed_index_errors() {
        assert!(parse_index("not json").is_err());
    }

    #[test]
    fn urls_are_raw_github() {
        assert_eq!(index_url(),
            "https://raw.githubusercontent.com/cavefish-dev/g13-driver/main/catalog/index.json");
        assert_eq!(profile_url("gaming.toml"),
            "https://raw.githubusercontent.com/cavefish-dev/g13-driver/main/catalog/gaming.toml");
    }

    #[test]
    fn mark_downloaded_joins_on_origin() {
        let entries = vec![
            CatalogEntry { filename: "a.toml".into(), name: "A".into() },
            CatalogEntry { filename: "b.toml".into(), name: "B".into() },
        ];
        let mut local = HashSet::new();
        local.insert("a.toml".to_string());
        let marked = mark_downloaded(entries, &local);
        assert_eq!(marked[0].1, true);  // a.toml downloaded
        assert_eq!(marked[1].1, false); // b.toml not
    }

    #[test]
    fn stamp_download_sets_provenance() {
        let mut p = crate::config::Profile::from_raw(
            toml::from_str("[meta]\nname = \"Gaming\"\n[keys]\nG1 = \"1\"\n").unwrap()).unwrap();
        stamp_download(&mut p, "gaming.toml");
        assert_eq!(p.source(), crate::config::ProfileSource::Github);
        assert_eq!(p.origin(), Some("gaming.toml"));
        assert!(!p.modified());
        assert_eq!(p.meta_name(), Some("Gaming")); // upstream name preserved
    }

    #[test]
    fn parse_profile_accepts_valid_rejects_garbage() {
        let good = parse_profile("[meta]\nname = \"X\"\n[keys]\nG1 = \"a\"\n", "x.toml");
        assert!(good.is_ok());
        let bad = parse_profile("this is not toml {{{", "x.toml");
        assert!(bad.is_err());
    }
}
