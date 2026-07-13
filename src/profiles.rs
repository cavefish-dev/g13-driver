use std::path::Path;
use crate::config::RawConfig;

/// One profile file in the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileEntry {
    pub filename: String,
    pub display_name: String,
}

/// Turn a display name into a filesystem-safe stem (no extension).
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in name.trim().to_lowercase().chars() {
        let mapped = if ch.is_ascii_alphanumeric() {
            Some(ch)
        } else if ch == ' ' || ch == '_' || ch == '-' {
            Some('-')
        } else {
            None
        };
        match mapped {
            Some('-') => {
                if !prev_dash && !out.is_empty() { out.push('-'); prev_dash = true; }
            }
            Some(c) => { out.push(c); prev_dash = false; }
            None => {}
        }
    }
    while out.ends_with('-') { out.pop(); }
    if out.is_empty() { "profile".to_string() } else { out }
}

/// A `<slug>.toml` name not already present in `dir` (suffixes -2, -3, …).
pub fn unique_filename(dir: &Path, display_name: &str) -> String {
    let base = slug(display_name);
    let mut candidate = format!("{base}.toml");
    let mut n = 2;
    while dir.join(&candidate).exists() {
        candidate = format!("{base}-{n}.toml");
        n += 1;
    }
    candidate
}

/// Lenient read of `[meta].name` from a profile file; None on any error or empty.
fn read_display_name(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let raw: RawConfig = toml::from_str(&text).ok()?;
    raw.meta
        .and_then(|m| m.name)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// All `.toml` files in `dir` as entries, sorted by display name (case-insensitive).
pub fn list(dir: &Path) -> Vec<ProfileEntry> {
    let mut entries = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let path = e.path();
            let Some(fname) = path.file_name().and_then(|s| s.to_str()) else { continue };
            if !fname.ends_with(".toml") { continue; }
            let stem = fname.trim_end_matches(".toml").to_string();
            let display_name = read_display_name(&path).unwrap_or(stem);
            entries.push(ProfileEntry { filename: fname.to_string(), display_name });
        }
    }
    entries.sort_by(|a, b| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase()));
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("g13-prof-{tag}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn slug_basic() {
        assert_eq!(slug("Basic"), "basic");
        assert_eq!(slug("My Games!"), "my-games");
        assert_eq!(slug("a__b  c"), "a-b-c");
        assert_eq!(slug("  trim--me  "), "trim-me");
    }

    #[test]
    fn slug_empty_falls_back() {
        assert_eq!(slug(""), "profile");
        assert_eq!(slug("🎮"), "profile");
    }

    #[test]
    fn unique_filename_suffixes_on_collision() {
        let d = tmp("unique");
        assert_eq!(unique_filename(&d, "Basic"), "basic.toml");
        std::fs::write(d.join("basic.toml"), "[keys]\n").unwrap();
        assert_eq!(unique_filename(&d, "Basic"), "basic-2.toml");
        std::fs::write(d.join("basic-2.toml"), "[keys]\n").unwrap();
        assert_eq!(unique_filename(&d, "Basic"), "basic-3.toml");
    }

    #[test]
    fn list_resolves_meta_then_stem_and_sorts() {
        let d = tmp("list");
        std::fs::write(d.join("zeta.toml"), "[meta]\nname = \"Alpha\"\n[keys]\n").unwrap();
        std::fs::write(d.join("beta.toml"), "[keys]\nG1 = \"a\"\n").unwrap(); // no meta -> stem
        std::fs::write(d.join("notes.txt"), "ignore me").unwrap();
        let entries = list(&d);
        // Sorted by display name: "Alpha" (zeta.toml) then "beta" (beta.toml).
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].display_name, "Alpha");
        assert_eq!(entries[0].filename, "zeta.toml");
        assert_eq!(entries[1].display_name, "beta");
        assert_eq!(entries[1].filename, "beta.toml");
    }
}
