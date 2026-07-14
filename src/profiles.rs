use std::path::Path;
use crate::config::RawConfig;
use anyhow::{Context, Result};
use crate::config::Profile;

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

/// Create a blank profile named `display_name`. Returns the new filename.
pub fn create(dir: &Path, display_name: &str) -> Result<String> {
    let filename = unique_filename(dir, display_name);
    let mut profile = Profile::default();
    profile.set_meta_name(Some(display_name.to_string()));
    std::fs::write(dir.join(&filename), profile.to_toml()?)
        .with_context(|| format!("failed to write {filename}"))?;
    Ok(filename)
}

/// Duplicate `src_filename` under a new name. Returns the new filename.
pub fn duplicate(dir: &Path, src_filename: &str, new_display_name: &str) -> Result<String> {
    let mut profile = Profile::load(&dir.join(src_filename))
        .with_context(|| format!("failed to load {src_filename}"))?;
    profile.set_meta_name(Some(new_display_name.to_string()));
    let filename = unique_filename(dir, new_display_name);
    std::fs::write(dir.join(&filename), profile.to_toml()?)
        .with_context(|| format!("failed to write {filename}"))?;
    Ok(filename)
}

/// Change ONLY `[meta].name`, preserving the rest of the file (bindings, comments).
pub fn rename(dir: &Path, filename: &str, new_display_name: &str) -> Result<()> {
    use toml_edit::{DocumentMut, Item, Table, value as toml_value};
    let path = dir.join(filename);
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {filename}"))?;
    let mut doc = text.parse::<DocumentMut>()
        .with_context(|| format!("failed to parse {filename}"))?;
    if !doc.as_table().contains_key("meta") {
        doc.as_table_mut().insert("meta", Item::Table(Table::new()));
    }
    doc["meta"]["name"] = toml_value(new_display_name);
    std::fs::write(&path, doc.to_string())
        .with_context(|| format!("failed to write {filename}"))?;
    Ok(())
}

/// Delete a profile file.
pub fn delete(dir: &Path, filename: &str) -> Result<()> {
    std::fs::remove_file(dir.join(filename))
        .with_context(|| format!("failed to delete {filename}"))?;
    Ok(())
}

use crate::protocol::MKey;

/// What deleting a profile entails: which M2/M3 slots must be cleared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletionPlan {
    pub unassign: Vec<MKey>,
}

/// Which M-slots must be cleared when `filename` is deleted (all that reference it).
/// Deletion is always allowed — an empty slot set is a valid state.
pub fn deletion_plan(filename: &str, slots: [Option<&str>; 3]) -> DeletionPlan {
    let mkeys = [MKey::M1, MKey::M2, MKey::M3];
    let unassign = slots.iter().zip(mkeys)
        .filter(|(s, _)| **s == Some(filename))
        .map(|(_, m)| m)
        .collect();
    DeletionPlan { unassign }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopyReport {
    pub copied: usize,
    pub skipped: usize,
}

/// Copy every `.toml` from `src_dir` into `dst_dir`, skipping names already present.
pub fn copy_into(src_dir: &Path, dst_dir: &Path) -> Result<CopyReport> {
    let mut copied = 0;
    let mut skipped = 0;
    if src_dir == dst_dir {
        return Ok(CopyReport { copied, skipped });
    }
    std::fs::create_dir_all(dst_dir)
        .with_context(|| format!("failed to create {}", dst_dir.display()))?;
    if let Ok(rd) = std::fs::read_dir(src_dir) {
        for e in rd.flatten() {
            let path = e.path();
            let Some(fname) = path.file_name().and_then(|s| s.to_str()) else { continue };
            if !fname.ends_with(".toml") { continue; }
            let target = dst_dir.join(fname);
            if target.exists() { skipped += 1; continue; }
            std::fs::copy(&path, &target)
                .with_context(|| format!("failed to copy {fname}"))?;
            copied += 1;
        }
    }
    Ok(CopyReport { copied, skipped })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::MKey;

    #[test]
    fn deletion_unassigns_every_referencing_slot() {
        let plan = deletion_plan("media.toml",
            [Some("basic.toml"), Some("media.toml"), Some("media.toml")]);
        assert_eq!(plan.unassign, vec![MKey::M2, MKey::M3]);
    }

    #[test]
    fn deletion_unassigns_m1_when_bound_there() {
        let plan = deletion_plan("basic.toml",
            [Some("basic.toml"), None, None]);
        assert_eq!(plan.unassign, vec![MKey::M1]);
    }

    #[test]
    fn deletion_of_unreferenced_profile_unassigns_nothing() {
        let plan = deletion_plan("extra.toml",
            [Some("basic.toml"), Some("media.toml"), None]);
        assert!(plan.unassign.is_empty());
    }

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

    #[test]
    fn create_writes_blank_profile_with_name() {
        let d = tmp("create");
        let fname = create(&d, "Fresh Start").unwrap();
        assert_eq!(fname, "fresh-start.toml");
        let p = crate::config::Profile::load(&d.join(&fname)).unwrap();
        assert_eq!(p.meta_name(), Some("Fresh Start"));
        assert_eq!(p.bindings().len(), 0);
    }

    #[test]
    fn duplicate_copies_bindings_under_new_name() {
        let d = tmp("dup");
        std::fs::write(d.join("src.toml"), "[meta]\nname = \"Src\"\n[keys]\nG1 = \"ctrl+c\"\n").unwrap();
        let fname = duplicate(&d, "src.toml", "Copy of Src").unwrap();
        assert_eq!(fname, "copy-of-src.toml");
        let p = crate::config::Profile::load(&d.join(&fname)).unwrap();
        assert_eq!(p.meta_name(), Some("Copy of Src"));
        assert_eq!(p.get_binding(crate::protocol::G13Key::G1), Some("ctrl+c"));
    }

    #[test]
    fn rename_changes_only_meta_name_and_keeps_bindings_and_file() {
        let d = tmp("rename");
        std::fs::write(d.join("keep.toml"),
            "# a comment\n[keys]\nG1 = \"ctrl+c\"\n").unwrap();
        rename(&d, "keep.toml", "Renamed").unwrap();
        assert!(d.join("keep.toml").exists(), "filename unchanged");
        let text = std::fs::read_to_string(d.join("keep.toml")).unwrap();
        assert!(text.contains("Renamed"));
        assert!(text.contains("# a comment"), "comment preserved");
        let p = crate::config::Profile::load(&d.join("keep.toml")).unwrap();
        assert_eq!(p.meta_name(), Some("Renamed"));
        assert_eq!(p.get_binding(crate::protocol::G13Key::G1), Some("ctrl+c"));
    }

    #[test]
    fn delete_removes_file() {
        let d = tmp("delete");
        std::fs::write(d.join("gone.toml"), "[keys]\n").unwrap();
        delete(&d, "gone.toml").unwrap();
        assert!(!d.join("gone.toml").exists());
    }

    #[test]
    fn copy_into_copies_and_skips_collisions() {
        let src = tmp("copy-src");
        let dst = tmp("copy-dst");
        std::fs::write(src.join("a.toml"), "[keys]\n").unwrap();
        std::fs::write(src.join("b.toml"), "[keys]\n").unwrap();
        std::fs::write(dst.join("b.toml"), "[keys]\nG1 = \"existing\"\n").unwrap();
        let report = copy_into(&src, &dst).unwrap();
        assert_eq!(report.copied, 1); // a.toml
        assert_eq!(report.skipped, 1); // b.toml already present
        // b.toml in dst is NOT overwritten.
        assert!(std::fs::read_to_string(dst.join("b.toml")).unwrap().contains("existing"));
        assert!(dst.join("a.toml").exists());
    }
}
