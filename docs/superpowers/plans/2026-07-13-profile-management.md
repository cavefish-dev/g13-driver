# Profile Management Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the read-only Profiles tab into a full profile manager — a relocatable, openable
library of `.toml` profiles with create / duplicate / rename / delete, assignable to the three
M-key slots — and ship exactly two bundled profiles (basic + media).

**Architecture:** A new pure `src/profiles.rs` owns the file library (list/create/duplicate/rename/
delete/copy, filename-slug generation); `ProfileSet` (in `config.rs`) gains a friendly `[meta].name`
field plus format-preserving manifest mutators (`persist_slot`, `persist_profiles_dir`); `runtime`
gains a synchronous `reload_now` and re-points the file watcher when the folder changes; the
Profiles tab in `src/monitor/mod.rs` is rebuilt to drive all of it. Disk is the source of truth —
every mutation writes to disk then reloads the shared `ProfileSet`.

**Tech Stack:** Rust (GNU toolchain), eframe/egui 0.31 (`egui::Modal`), `toml` + `toml_edit`,
`serde`, `notify`, `rfd` (native folder picker), `explorer.exe` (open folder).

## Global Constraints

- **Toolchain:** GNU only (`stable-x86_64-pc-windows-gnu`), NO MSVC. If a build/link error appears,
  prepend PATH: `export PATH="$HOME/.cargo/bin:/c/Strawberry/c/bin:$PATH"`.
- **Binary crate:** run `cargo test` (NEVER `cargo test --lib` — it silently skips the binary's tests).
- **TDD** for all pure logic (RED → GREEN). The documented exception (manual-verify, no unit tests):
  GUI code, the `rfd` picker, `explorer.exe`, and the live watcher re-point.
- **Error policy:** no `panic!`/`unwrap()`/`expect()` in the runtime or UI path; operations return
  `Result`, failures land in a UI status line and leave the app running on current state.
- **Platform isolation:** `rfd` and `explorer.exe` usage stays behind `#[cfg(windows)]`. `rfd` is
  declared under `[target.'cfg(windows)'.dependencies]`.
- **Profile schema:** friendly name lives in a `[meta]` table as `name = "..."`. Display name =
  `[meta].name` if present and non-empty, else the filename stem. Legacy files (no `[meta]`) need no
  migration.
- **Filename stability:** rename changes ONLY `[meta].name`; the filename never changes (the manifest
  references slots by filename, e.g. `m1 = "basic.toml"`).
- **Filename slug:** lowercase; spaces/`_` → `-`; drop anything outside `[a-z0-9-]`; collapse repeated
  `-`; trim leading/trailing `-`; empty result → `profile`; ensure uniqueness by suffixing `-2`,
  `-3`, …
- **Load invariant:** the manifest's `m1` must always resolve, or `ProfileSet::load` errors. Delete
  is refused when the target is bound to M1 or is the last profile; deleting an M2/M3-bound profile
  auto-unassigns that slot.
- **Ship exactly two profiles:** `basic.toml` (name "Basic") + `media.toml` (name "Media"); delete
  `default.toml` and `game.toml`. Default manifest: `m1 = "basic.toml"`, `m2 = "media.toml"`, no `m3`.
- **Commits:** one focused commit per step-group, imperative subject; trailer
  `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.

---

### Task 1: Profile `[meta].name` schema + `rfd` dependency de-risk

Add the friendly-name field to the profile schema (parse + serialize + stem-agnostic accessor) and
retire the `rfd` GNU-toolchain build risk up front by adding the dependency and confirming it compiles.

**Files:**
- Modify: `src/config.rs` (RawConfig, new RawMeta, Profile, `to_toml`)
- Modify: `Cargo.toml` (add `rfd` under the Windows target table)
- Test: `src/config.rs` (in the existing `#[cfg(test)] mod tests`)

**Interfaces:**
- Produces:
  - `Profile::meta_name(&self) -> Option<&str>` — the raw `[meta].name` (None when absent/empty).
  - `Profile::set_meta_name(&mut self, name: Option<String>)`.
  - `to_toml()` emits a leading `[meta]` table with `name` when `meta_name` is `Some`.
  - `RawConfig` and `RawMeta` are `pub(crate)` so `profiles.rs` can parse `[meta].name` leniently.

- [ ] **Step 1: Write the failing tests**

Add to `src/config.rs`, inside `#[cfg(test)] mod tests` (the second test module, near the bottom):

```rust
#[test]
fn parses_meta_name() {
    let src = "[meta]\nname = \"My Profile\"\n[keys]\nG1 = \"a\"\n";
    let raw: RawConfig = toml::from_str(src).unwrap();
    let p = Profile::from_raw(raw).unwrap();
    assert_eq!(p.meta_name(), Some("My Profile"));
}

#[test]
fn meta_name_absent_is_none() {
    let p = Profile::from_raw(raw(&[("G1", "a")])).unwrap();
    assert_eq!(p.meta_name(), None);
}

#[test]
fn empty_meta_name_is_none() {
    let src = "[meta]\nname = \"\"\n[keys]\nG1 = \"a\"\n";
    let raw: RawConfig = toml::from_str(src).unwrap();
    let p = Profile::from_raw(raw).unwrap();
    assert_eq!(p.meta_name(), None);
}

#[test]
fn to_toml_round_trips_meta_name() {
    let mut p = Profile::from_raw(raw(&[("G1", "a")])).unwrap();
    p.set_meta_name(Some("Basic".to_string()));
    let toml = p.to_toml().unwrap();
    assert!(toml.contains("[meta]"));
    let reloaded = Profile::from_raw(toml::from_str(&toml).unwrap()).unwrap();
    assert_eq!(reloaded.meta_name(), Some("Basic"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test parses_meta_name meta_name_absent_is_none empty_meta_name_is_none to_toml_round_trips_meta_name`
Expected: FAIL — `no method named meta_name` / `no field meta`.

- [ ] **Step 3: Implement the schema**

In `src/config.rs`:

Add the raw meta struct near `RawJoystick`:

```rust
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub(crate) struct RawMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}
```

Add a `meta` field to `RawConfig` **as the first field** (so `[meta]` serializes before `[keys]`),
and make the struct `pub(crate)`:

```rust
#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct RawConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<RawMeta>,
    #[serde(default)]
    pub keys: HashMap<String, String>,
    #[serde(default)]
    pub joystick: Option<RawJoystick>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub repeat: HashMap<String, bool>,
}
```

Add a `meta_name: Option<String>` field to `Profile`:

```rust
#[derive(Debug, Clone)]
pub struct Profile {
    key_bindings: HashMap<G13Key, String>,
    joystick: Option<JoystickConfig>,
    repeat: HashMap<G13Key, bool>,
    meta_name: Option<String>,
}
```

In `Profile::from_raw`, resolve the name (treat empty as None) and include it in the constructed
`Profile`:

```rust
        let meta_name = raw.meta
            .and_then(|m| m.name)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        Ok(Self { key_bindings, joystick, repeat, meta_name })
```

Add the accessors (near `bindings`/`set_bindings`):

```rust
    pub fn meta_name(&self) -> Option<&str> {
        self.meta_name.as_deref()
    }

    pub fn set_meta_name(&mut self, name: Option<String>) {
        self.meta_name = name.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    }
```

In `to_toml`, populate `meta` in the `RawConfig` it builds (find the `let raw = RawConfig { keys, joystick, repeat };` line):

```rust
        let meta = self.meta_name.clone().map(|name| RawMeta { name: Some(name) });
        let raw = RawConfig { meta, keys, joystick, repeat };
```

Every other place that constructs `RawConfig { ... }` (the test helper `raw()` in the same file)
must add `meta: None,` — update the `raw()` helper:

```rust
    fn raw(pairs: &[(&str, &str)]) -> RawConfig {
        RawConfig {
            meta: None,
            keys: pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            joystick: None,
            repeat: HashMap::new(),
        }
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test`
Expected: PASS — all prior tests still green + the 4 new ones. (Fix any other `RawConfig { … }`
construction sites the compiler flags by adding `meta: None,`.)

- [ ] **Step 5: Add `rfd` and confirm the GNU build**

In `Cargo.toml`, under `[target.'cfg(windows)'.dependencies]`, add:

```toml
# Native folder picker for the Profiles tab (src/monitor/mod.rs), Windows-only.
rfd = "0.15"
```

Run: `cargo build`
Expected: PASS — `rfd` and its transitive deps compile under MinGW gcc. If it fails to build,
**stop and report** (the spec's fallback is a path text field instead of the native picker).

- [ ] **Step 6: Commit**

```bash
git add src/config.rs Cargo.toml Cargo.lock
git commit -m "feat: add [meta].name profile schema + rfd dependency"
```

---

### Task 2: `profiles.rs` — `slug`, `unique_filename`, `list`

Create the library module and its read/inspect half.

**Files:**
- Create: `src/profiles.rs`
- Modify: `src/main.rs` (add `mod profiles;`)
- Test: `src/profiles.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `crate::config::{RawConfig}` (lenient `[meta].name` read).
- Produces:
  - `pub struct ProfileEntry { pub filename: String, pub display_name: String }`
  - `pub fn slug(name: &str) -> String`
  - `pub fn unique_filename(dir: &Path, display_name: &str) -> String` (returns `"<slug>.toml"`)
  - `pub fn list(dir: &Path) -> Vec<ProfileEntry>` (sorted by display name, case-insensitive)

- [ ] **Step 1: Write the failing tests**

Create `src/profiles.rs` with only the tests + a module skeleton:

```rust
use std::path::Path;

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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test slug_basic slug_empty_falls_back unique_filename_suffixes_on_collision list_resolves`
Expected: FAIL — `cannot find function slug` (and `mod profiles` not declared).

- [ ] **Step 3: Implement + register the module**

In `src/main.rs`, add alongside the other `mod` declarations:

```rust
mod profiles;
```

Add the implementation to `src/profiles.rs` (above the test module):

```rust
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test`
Expected: PASS — all green including the 4 new tests.

- [ ] **Step 5: Commit**

```bash
git add src/profiles.rs src/main.rs
git commit -m "feat: profiles library — slug, unique_filename, list"
```

---

### Task 3: `profiles.rs` — `create`, `duplicate`, `rename`, `delete`, `copy_into`

The mutating half of the library.

**Files:**
- Modify: `src/profiles.rs`
- Test: `src/profiles.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `crate::config::Profile` (`load`, `set_meta_name`, `to_toml`), `slug`, `unique_filename`.
- Produces:
  - `pub fn create(dir: &Path, display_name: &str) -> anyhow::Result<String>` (returns filename)
  - `pub fn duplicate(dir: &Path, src_filename: &str, new_display_name: &str) -> anyhow::Result<String>`
  - `pub fn rename(dir: &Path, filename: &str, new_display_name: &str) -> anyhow::Result<()>`
  - `pub fn delete(dir: &Path, filename: &str) -> anyhow::Result<()>`
  - `pub struct CopyReport { pub copied: usize, pub skipped: usize }`
  - `pub fn copy_into(src_dir: &Path, dst_dir: &Path) -> anyhow::Result<CopyReport>`

- [ ] **Step 1: Write the failing tests**

Add to `src/profiles.rs` `mod tests`:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test create_writes_blank duplicate_copies rename_changes delete_removes copy_into_copies`
Expected: FAIL — `cannot find function create`.

- [ ] **Step 3: Implement**

Add to `src/profiles.rs` (above `mod tests`):

```rust
use anyhow::{Context, Result};
use crate::config::Profile;

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
```

This needs `Profile::default()`. Add a `Default` impl to `Profile` in `src/config.rs` (near the
`impl Profile`):

```rust
impl Default for Profile {
    fn default() -> Self {
        Self {
            key_bindings: HashMap::new(),
            joystick: None,
            repeat: HashMap::new(),
            meta_name: None,
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test`
Expected: PASS — all green including the 5 new tests. Note `create` writes `[meta]` then `[keys]`
(empty) via `to_toml`; `Profile::load` of a blank keys table succeeds.

- [ ] **Step 5: Commit**

```bash
git add src/profiles.rs src/config.rs
git commit -m "feat: profiles library — create, duplicate, rename, delete, copy_into"
```

---

### Task 4: `ProfileSet` manifest mutators — `persist_slot`, `persist_profiles_dir`

Format-preserving writes of the slot assignments and folder path, mirroring `persist_start_active`.

**Files:**
- Modify: `src/config.rs` (`impl ProfileSet`)
- Test: `src/config.rs` (`#[cfg(test)] mod profileset_tests`)

**Interfaces:**
- Consumes: `crate::protocol::MKey`, `self.config_path`.
- Produces:
  - `pub fn persist_slot(&self, key: MKey, filename: Option<&str>) -> Result<()>` — sets `m1|m2|m3`
    to the filename, or removes the key when `None`. MR is a no-op `Ok(())`.
  - `pub fn persist_profiles_dir(&self, dir: &Path) -> Result<()>` — sets `profiles_dir` to the
    path (as a string; `dir.display()`).

- [ ] **Step 1: Write the failing tests**

Add to `src/config.rs` `mod profileset_tests`:

```rust
    #[test]
    fn persist_slot_sets_and_clears_preserving_others() {
        let d = tmp("persist-slot");
        write(&d.join("profiles"), "default.toml", "[keys]\nG1 = \"a\"\n");
        write(&d.join("profiles"), "media.toml", "[keys]\nG1 = \"space\"\n");
        write(&d, "config.toml",
            "# manifest\nprofiles_dir = \"profiles\"\nm1 = \"default.toml\"\nm2 = \"media.toml\"\n");
        let set = ProfileSet::load(&d.join("config.toml")).unwrap();

        // Set m2 -> default.toml, clear m3 (absent -> stays absent, no error).
        set.persist_slot(MKey::M2, Some("default.toml")).unwrap();
        set.persist_slot(MKey::M3, None).unwrap();

        let text = std::fs::read_to_string(d.join("config.toml")).unwrap();
        assert!(text.contains("m2 = \"default.toml\""));
        assert!(text.contains("# manifest"), "comment preserved");
        assert!(text.contains("m1 = \"default.toml\""), "m1 preserved");
        assert!(!text.contains("m3 ="), "m3 stays absent");
    }

    #[test]
    fn persist_slot_clear_removes_existing_key() {
        let d = tmp("persist-clear");
        write(&d.join("profiles"), "default.toml", "[keys]\nG1 = \"a\"\n");
        write(&d, "config.toml",
            "profiles_dir = \"profiles\"\nm1 = \"default.toml\"\nm2 = \"default.toml\"\n");
        let set = ProfileSet::load(&d.join("config.toml")).unwrap();
        set.persist_slot(MKey::M2, None).unwrap();
        let text = std::fs::read_to_string(d.join("config.toml")).unwrap();
        assert!(!text.contains("m2 ="), "m2 removed");
        assert!(text.contains("m1 = \"default.toml\""));
    }

    #[test]
    fn persist_profiles_dir_updates_and_reloads() {
        let d = tmp("persist-dir");
        std::fs::create_dir_all(d.join("elsewhere")).unwrap();
        write(&d.join("profiles"), "default.toml", "[keys]\nG1 = \"a\"\n");
        write(&d.join("elsewhere"), "default.toml", "[keys]\nG1 = \"b\"\n");
        write(&d, "config.toml", "profiles_dir = \"profiles\"\nm1 = \"default.toml\"\n");
        let set = ProfileSet::load(&d.join("config.toml")).unwrap();
        set.persist_profiles_dir(&d.join("elsewhere")).unwrap();

        let reloaded = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert_eq!(reloaded.active_profile().get_binding(crate::protocol::G13Key::G1), Some("b"));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test persist_slot_sets persist_slot_clear persist_profiles_dir_updates`
Expected: FAIL — `no method named persist_slot`.

- [ ] **Step 3: Implement**

Add to `impl ProfileSet` in `src/config.rs` (near `persist_start_active`):

```rust
    /// Set (`Some`) or clear (`None`) an M-slot in the manifest, preserving all other
    /// keys and comments. MR is a no-op.
    pub fn persist_slot(&self, key: MKey, filename: Option<&str>) -> Result<()> {
        use toml_edit::{DocumentMut, value as toml_value};
        let slot = match key {
            MKey::M1 => "m1",
            MKey::M2 => "m2",
            MKey::M3 => "m3",
            MKey::MR => return Ok(()),
        };
        let text = std::fs::read_to_string(&self.config_path)
            .with_context(|| format!("failed to read {}", self.config_path.display()))?;
        let mut doc = text.parse::<DocumentMut>()
            .with_context(|| format!("failed to parse {}", self.config_path.display()))?;
        match filename {
            Some(name) => { doc[slot] = toml_value(name); }
            None => { doc.as_table_mut().remove(slot); }
        }
        std::fs::write(&self.config_path, doc.to_string())
            .with_context(|| format!("failed to write {}", self.config_path.display()))?;
        Ok(())
    }

    /// Set `profiles_dir` in the manifest, preserving all other keys and comments.
    pub fn persist_profiles_dir(&self, dir: &Path) -> Result<()> {
        use toml_edit::{DocumentMut, value as toml_value};
        let text = std::fs::read_to_string(&self.config_path)
            .with_context(|| format!("failed to read {}", self.config_path.display()))?;
        let mut doc = text.parse::<DocumentMut>()
            .with_context(|| format!("failed to parse {}", self.config_path.display()))?;
        doc["profiles_dir"] = toml_value(dir.display().to_string());
        std::fs::write(&self.config_path, doc.to_string())
            .with_context(|| format!("failed to write {}", self.config_path.display()))?;
        Ok(())
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test`
Expected: PASS — all green including the 3 new tests.

- [ ] **Step 5: Commit**

```bash
git add src/config.rs
git commit -m "feat: manifest mutators — persist_slot, persist_profiles_dir"
```

---

### Task 5: `runtime` — `reload_now` + watcher re-point on folder change

Synchronous reload for immediate UI feedback, and teach the watcher to follow the folder.

**Files:**
- Modify: `src/runtime.rs` (`reload_now` + `watch_config`)
- Test: `src/runtime.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Produces:
  - `pub fn reload_now(config: &Arc<RwLock<ProfileSet>>, config_path: &Path) -> anyhow::Result<()>`
    — loads a fresh `ProfileSet`, preserves the current active M-key when its slot resolves, swaps
    under the write lock.
- Behavior change: `watch_config` re-derives the watched profiles dir after each reload and swaps
  the watch when `profiles_dir` changed (manual-verify, no unit test).

- [ ] **Step 1: Write the failing test**

Add to `src/runtime.rs` a test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn tmp(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("g13-rt-{tag}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("profiles")).unwrap();
        d
    }

    #[test]
    fn reload_now_picks_up_disk_changes() {
        let d = tmp("reload");
        std::fs::write(d.join("profiles/default.toml"), "[keys]\nG1 = \"a\"\n").unwrap();
        std::fs::write(d.join("config.toml"),
            "profiles_dir = \"profiles\"\nm1 = \"default.toml\"\n").unwrap();
        let cfg = std::sync::Arc::new(std::sync::RwLock::new(
            ProfileSet::load(&d.join("config.toml")).unwrap()));

        // Change the profile file on disk, then reload_now.
        std::fs::write(d.join("profiles/default.toml"), "[keys]\nG1 = \"z\"\n").unwrap();
        reload_now(&cfg, &d.join("config.toml")).unwrap();
        assert_eq!(
            cfg.read().unwrap().active_profile().get_binding(crate::protocol::G13Key::G1),
            Some("z"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test reload_now_picks_up_disk_changes`
Expected: FAIL — `cannot find function reload_now`.

- [ ] **Step 3: Implement `reload_now` and re-point the watcher**

Add to `src/runtime.rs`:

```rust
use std::path::Path;

/// Reload the ProfileSet from disk and swap it under the write lock, preserving the
/// active M-key when its slot still resolves.
pub fn reload_now(config: &Arc<RwLock<ProfileSet>>, config_path: &Path) -> Result<()> {
    let active = config.read().unwrap().active();
    let mut new = ProfileSet::load(config_path)?;
    new.set_active(active); // no-op if that slot is now empty; stays on M1
    *config.write().unwrap() = new;
    Ok(())
}
```

Update `watch_config` so the watched profiles dir follows a folder change. Replace the reload loop
body:

```rust
    let mut watched_dir = profiles_dir;
    for result in rx {
        if result.is_ok() {
            let active = config.read().unwrap().active();
            match ProfileSet::load(&config_path) {
                Ok(mut new) => {
                    new.set_active(active);
                    let new_dir = new.profiles_dir().to_path_buf();
                    *config.write().unwrap() = new;
                    if new_dir != watched_dir {
                        let _ = watcher.unwatch(&watched_dir);
                        let _ = watcher.watch(&new_dir, RecursiveMode::Recursive);
                        watched_dir = new_dir;
                        log::info!("watching profiles dir {}", watched_dir.display());
                    }
                    log::info!("config reloaded");
                }
                Err(e) => log::warn!("config reload failed: {e:#}"),
            }
        }
    }
```

(The `watcher` binding must remain in scope for the loop — it already does; ensure it's `let mut
watcher` so `unwatch`/`watch` are callable. It is already `let mut watcher` in the current code.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test`
Expected: PASS — all green including `reload_now_picks_up_disk_changes`.

- [ ] **Step 5: Commit**

```bash
git add src/runtime.rs
git commit -m "feat: reload_now + watcher re-points on folder change"
```

---

### Task 6: Delete-guardrail logic (pure, GUI-free)

Extract the "can I delete this, and what does it cascade" decision so it's testable without egui.

**Files:**
- Modify: `src/profiles.rs` (add `deletion_plan`)
- Test: `src/profiles.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `crate::protocol::MKey`.
- Produces:
  - `pub struct DeletionPlan { pub unassign: Vec<MKey> }`
  - `pub fn deletion_plan(filename: &str, slots: [Option<&str>; 3], total_profiles: usize)
    -> Result<DeletionPlan, String>` where `slots` is `[m1, m2, m3]` filenames. Returns `Err(reason)`
    when the file is bound to M1 or is the last profile; otherwise `Ok` with the M2/M3 slots that
    referenced it (to be cleared).

- [ ] **Step 1: Write the failing tests**

Add to `src/profiles.rs` `mod tests`:

```rust
    use crate::protocol::MKey;

    #[test]
    fn deletion_refused_when_bound_to_m1() {
        let err = deletion_plan("basic.toml",
            [Some("basic.toml"), Some("media.toml"), None], 2).unwrap_err();
        assert!(err.to_lowercase().contains("m1"));
    }

    #[test]
    fn deletion_refused_when_last_profile() {
        let err = deletion_plan("only.toml",
            [Some("only.toml"), None, None], 1).unwrap_err();
        assert!(err.to_lowercase().contains("only"));
    }

    #[test]
    fn deletion_unassigns_m2_and_m3() {
        let plan = deletion_plan("media.toml",
            [Some("basic.toml"), Some("media.toml"), Some("media.toml")], 2).unwrap();
        assert_eq!(plan.unassign, vec![MKey::M2, MKey::M3]);
    }

    #[test]
    fn deletion_of_unassigned_profile_is_clean() {
        let plan = deletion_plan("extra.toml",
            [Some("basic.toml"), Some("media.toml"), None], 3).unwrap();
        assert!(plan.unassign.is_empty());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test deletion_refused deletion_unassigns deletion_of_unassigned`
Expected: FAIL — `cannot find function deletion_plan`.

- [ ] **Step 3: Implement**

Add to `src/profiles.rs`:

```rust
use crate::protocol::MKey;

/// What deleting a profile entails: which M2/M3 slots must be cleared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletionPlan {
    pub unassign: Vec<MKey>,
}

/// Decide whether `filename` may be deleted. `slots` is `[m1, m2, m3]` filenames.
/// Refuses when bound to M1 or when it is the last profile in the folder.
pub fn deletion_plan(
    filename: &str,
    slots: [Option<&str>; 3],
    total_profiles: usize,
) -> Result<DeletionPlan, String> {
    if slots[0] == Some(filename) {
        return Err("This profile is assigned to M1. Reassign M1 first.".to_string());
    }
    if total_profiles <= 1 {
        return Err("Can't delete the only profile.".to_string());
    }
    let mut unassign = Vec::new();
    if slots[1] == Some(filename) { unassign.push(MKey::M2); }
    if slots[2] == Some(filename) { unassign.push(MKey::M3); }
    Ok(DeletionPlan { unassign })
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test`
Expected: PASS — all green including the 4 new tests.

- [ ] **Step 5: Commit**

```bash
git add src/profiles.rs
git commit -m "feat: profiles deletion_plan guardrail logic"
```

---

### Task 7: Rebuilt Profiles tab UI

Wire the library, manifest mutators, reload, guardrail, `rfd`, and `explorer.exe` into the tab.
GUI + OS integration — **manual-verify, no unit tests** (documented exception).

**Files:**
- Modify: `src/monitor/mod.rs` (MonitorApp fields, `render_profiles` rewrite, new imports)

**Interfaces:**
- Consumes: `crate::profiles::{self, ProfileEntry, deletion_plan}`,
  `crate::runtime::reload_now`, `ProfileSet::{persist_slot, persist_profiles_dir}`,
  `crate::protocol::MKey`, `config_path` (see Step 1).

- [ ] **Step 1: Give the app the config path + new UI state**

`render_profiles` needs the config file path for `reload_now`. Add a field to `MonitorApp` and
thread it through `MonitorApp::new` / `run_monitor`.

In `src/monitor/mod.rs`, add fields to `struct MonitorApp`:

```rust
    config_path: std::path::PathBuf,
    name_prompt: Option<NamePrompt>,
    pending_delete: Option<String>,
    profiles_status: Option<String>,
```

Add the prompt type near the `Tab` enum:

```rust
#[derive(Clone)]
enum PromptKind { New, Duplicate { src: String }, Rename { filename: String } }

#[derive(Clone)]
struct NamePrompt {
    kind: PromptKind,
    buffer: String,
}
```

Thread `config_path` in: the GUI entry point is `pub fn run(config, start_minimized)` in
`src/monitor/mod.rs` (called as `monitor::run(config, minimized)` at `src/main.rs:73` and `:78`).
Add a `config_path: PathBuf` parameter to `monitor::run` and to `MonitorApp::new`, initialize the
new fields (`name_prompt: None, pending_delete: None, profiles_status: None`). In `src/main.rs`,
`resolve_config_path()` is currently called inline at `:52` and moved into
`load_config_and_watch`; capture it in a `let config_path = resolve_config_path();` first, pass
`config_path.clone()` to `load_config_and_watch`, and pass `config_path` to both `monitor::run`
call sites.

Build check: `cargo build` (compiles with the fields unused for now is fine — they're used next).

- [ ] **Step 2: Rewrite `render_profiles`**

Replace the whole `fn render_profiles(&self, ui: &mut egui::Ui)` (note: signature becomes `&mut
self`) with the manager. Full body:

```rust
    fn render_profiles(&mut self, ui: &mut egui::Ui) {
        ui.heading("Profiles");
        ui.label("Click a slot to make it active, then click a profile below to assign it to that slot.");
        ui.add_space(6.0);

        // Snapshot state under a short read lock.
        let (active, slot_names, dir) = {
            let set = self.profiles.read().unwrap();
            let names = [
                set.name(MKey::M1).map(String::from),
                set.name(MKey::M2).map(String::from),
                set.name(MKey::M3).map(String::from),
            ];
            (set.active(), names, set.profiles_dir().to_path_buf())
        };
        let entries = crate::profiles::list(&dir);
        let display_of = |filename: &str| -> String {
            entries.iter().find(|e| e.filename == filename)
                .map(|e| e.display_name.clone())
                .unwrap_or_else(|| filename.trim_end_matches(".toml").to_string())
        };

        // --- Slots ---
        let mkeys = [MKey::M1, MKey::M2, MKey::M3];
        let mut switch_to: Option<MKey> = None;
        for (i, m) in mkeys.iter().enumerate() {
            let label = match &slot_names[i] {
                Some(f) => format!("{m:?}  —  {}", display_of(f)),
                None => format!("{m:?}  —  (unassigned)"),
            };
            if ui.selectable_label(*m == active, label).clicked() {
                switch_to = Some(*m);
            }
        }
        if let Some(m) = switch_to {
            self.profiles.write().unwrap().set_active(m);
        }

        ui.add_space(10.0);
        ui.separator();

        // --- Folder bar ---
        ui.horizontal(|ui| {
            ui.label("Folder:");
            ui.monospace(elide_path(&dir, 48));
        });
        ui.horizontal(|ui| {
            if ui.button("Change folder…").clicked() {
                self.change_folder(&dir);
            }
            if ui.button("Open folder").clicked() {
                open_folder(&dir);
            }
            if ui.button("New").clicked() {
                self.name_prompt = Some(NamePrompt { kind: PromptKind::New, buffer: String::new() });
            }
        });

        ui.add_space(8.0);

        // --- Library list ---
        let active_slot_file = slot_index(active).and_then(|i| slot_names[i].clone());
        egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
            for e in &entries {
                ui.horizontal(|ui| {
                    let is_active_file = active_slot_file.as_deref() == Some(e.filename.as_str());
                    if ui.selectable_label(is_active_file, &e.display_name).clicked() {
                        self.assign_to_active(&e.filename);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Delete").clicked() {
                            self.try_begin_delete(&e.filename, &dir, &entries);
                        }
                        if ui.small_button("Rename").clicked() {
                            self.name_prompt = Some(NamePrompt {
                                kind: PromptKind::Rename { filename: e.filename.clone() },
                                buffer: e.display_name.clone(),
                            });
                        }
                        if ui.small_button("Duplicate").clicked() {
                            self.name_prompt = Some(NamePrompt {
                                kind: PromptKind::Duplicate { src: e.filename.clone() },
                                buffer: format!("Copy of {}", e.display_name),
                            });
                        }
                    });
                });
            }
        });

        if let Some(s) = &self.profiles_status {
            ui.add_space(6.0);
            ui.weak(s);
        }

        self.render_name_prompt(ui.ctx(), &dir);
        self.render_delete_confirm(ui.ctx(), &dir, &entries);
    }
```

The render body above uses `slot_index(active)` to find the active slot's filename without depending
on the enum's discriminant values. Add this helper at module scope (bottom of
`src/monitor/mod.rs`):

```rust
fn slot_index(m: MKey) -> Option<usize> {
    match m { MKey::M1 => Some(0), MKey::M2 => Some(1), MKey::M3 => Some(2), MKey::MR => None }
}
```

**Borrow-checker note:** the existing `render_profiles` already uses a deferred-action pattern
(`switch_to: Option<MKey>`, applied after the loop) because egui's `show`/`horizontal` closures
borrow `ui`. The library-list handlers here call `&mut self` methods (`assign_to_active`,
`try_begin_delete`) directly inside nested closures. If the borrow checker rejects that, generalize
the `switch_to` pattern: collect the intended action into a local `enum Action { Assign(String),
Delete(String), Prompt(NamePrompt) }` variable inside the closures, then apply it after the
`ScrollArea` closure returns (where `self` is freely mutable). Do not hold a `profiles` read/write
lock across an egui closure.

- [ ] **Step 3: Add the action methods**

Add to `impl MonitorApp`:

```rust
    fn assign_to_active(&mut self, filename: &str) {
        let active = self.profiles.read().unwrap().active();
        let res = self.profiles.read().unwrap().persist_slot(active, Some(filename))
            .and_then(|_| crate::runtime::reload_now(&self.profiles, &self.config_path));
        self.profiles_status = Some(match res {
            Ok(()) => format!("Assigned to {active:?}."),
            Err(e) => format!("Assign failed: {e}"),
        });
    }

    fn change_folder(&mut self, current: &std::path::Path) {
        #[cfg(windows)]
        {
            let picked = rfd::FileDialog::new().set_directory(current).pick_folder();
            let Some(new_dir) = picked else { return };
            let res = (|| -> anyhow::Result<crate::profiles::CopyReport> {
                let report = crate::profiles::copy_into(current, &new_dir)?;
                self.profiles.read().unwrap().persist_profiles_dir(&new_dir)?;
                crate::runtime::reload_now(&self.profiles, &self.config_path)?;
                Ok(report)
            })();
            self.profiles_status = Some(match res {
                Ok(r) => format!("Folder changed. Copied {} profile(s), skipped {}.", r.copied, r.skipped),
                Err(e) => format!("Change folder failed: {e}"),
            });
        }
        #[cfg(not(windows))]
        { let _ = current; }
    }

    fn try_begin_delete(&mut self, filename: &str, dir: &std::path::Path, entries: &[crate::profiles::ProfileEntry]) {
        let set = self.profiles.read().unwrap();
        let slots = [set.name(MKey::M1), set.name(MKey::M2), set.name(MKey::M3)];
        match crate::profiles::deletion_plan(filename, slots, entries.len()) {
            Ok(_) => { drop(set); self.pending_delete = Some(filename.to_string()); }
            Err(reason) => { drop(set); self.profiles_status = Some(reason); }
        }
    }

    fn render_name_prompt(&mut self, ctx: &egui::Context, dir: &std::path::Path) {
        let Some(mut prompt) = self.name_prompt.take() else { return };
        let mut open = true;
        let mut submit = false;
        egui::Modal::new(egui::Id::new("name_prompt")).show(ctx, |ui| {
            ui.set_width(320.0);
            let title = match &prompt.kind {
                PromptKind::New => "New profile",
                PromptKind::Duplicate { .. } => "Duplicate profile",
                PromptKind::Rename { .. } => "Rename profile",
            };
            ui.heading(title);
            ui.add_space(4.0);
            let resp = ui.text_edit_singleline(&mut prompt.buffer);
            resp.request_focus();
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let valid = !prompt.buffer.trim().is_empty();
                if ui.add_enabled(valid, egui::Button::new("OK")).clicked() { submit = true; }
                if ui.button("Cancel").clicked() { open = false; }
            });
        });
        if submit {
            let name = prompt.buffer.trim().to_string();
            let res: anyhow::Result<()> = (|| {
                match &prompt.kind {
                    PromptKind::New => { crate::profiles::create(dir, &name)?; }
                    PromptKind::Duplicate { src } => { crate::profiles::duplicate(dir, src, &name)?; }
                    PromptKind::Rename { filename } => { crate::profiles::rename(dir, filename, &name)?; }
                }
                crate::runtime::reload_now(&self.profiles, &self.config_path)
            })();
            self.profiles_status = Some(match res {
                Ok(()) => "Saved.".to_string(),
                Err(e) => format!("Failed: {e}"),
            });
            // fall through: prompt consumed (not re-stored)
        } else if open {
            self.name_prompt = Some(prompt); // keep showing until OK/Cancel
        }
    }

    fn render_delete_confirm(&mut self, ctx: &egui::Context, dir: &std::path::Path, entries: &[crate::profiles::ProfileEntry]) {
        let Some(filename) = self.pending_delete.clone() else { return };
        let display = entries.iter().find(|e| e.filename == filename)
            .map(|e| e.display_name.clone()).unwrap_or_else(|| filename.clone());
        let mut confirm = false;
        let mut cancel = false;
        egui::Modal::new(egui::Id::new("delete_confirm")).show(ctx, |ui| {
            ui.set_width(320.0);
            ui.heading("Delete profile");
            ui.label(format!("Delete profile '{display}'? This removes the file."));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Delete").clicked() { confirm = true; }
                if ui.button("Cancel").clicked() { cancel = true; }
            });
        });
        if confirm {
            let res: anyhow::Result<()> = (|| {
                // Re-evaluate the plan against current state, then cascade unassigns.
                let (slots_owned, total) = {
                    let set = self.profiles.read().unwrap();
                    ([set.name(MKey::M1).map(String::from),
                      set.name(MKey::M2).map(String::from),
                      set.name(MKey::M3).map(String::from)], entries.len())
                };
                let slots = [slots_owned[0].as_deref(), slots_owned[1].as_deref(), slots_owned[2].as_deref()];
                let plan = crate::profiles::deletion_plan(&filename, slots, total)
                    .map_err(|e| anyhow::anyhow!(e))?;
                {
                    let set = self.profiles.read().unwrap();
                    for m in &plan.unassign { set.persist_slot(*m, None)?; }
                }
                crate::profiles::delete(dir, &filename)?;
                crate::runtime::reload_now(&self.profiles, &self.config_path)
            })();
            self.profiles_status = Some(match res {
                Ok(()) => "Deleted.".to_string(),
                Err(e) => format!("Delete failed: {e}"),
            });
            self.pending_delete = None;
        } else if cancel {
            self.pending_delete = None;
        }
    }
```

Add module-scope helpers (bottom of file):

```rust
fn elide_path(p: &std::path::Path, max: usize) -> String {
    let s = p.display().to_string();
    if s.chars().count() <= max { s } else {
        let tail: String = s.chars().rev().take(max - 1).collect::<Vec<_>>().into_iter().rev().collect();
        format!("…{tail}")
    }
}

#[cfg(windows)]
fn open_folder(dir: &std::path::Path) {
    let _ = std::process::Command::new("explorer").arg(dir).spawn();
}
#[cfg(not(windows))]
fn open_folder(_dir: &std::path::Path) {}
```

Update the tab dispatch: `Tab::Profiles => self.render_profiles(ui),` still compiles (now `&mut
self`). Ensure the `update()` match arm borrows `self` mutably — it already calls other `&mut self`
renderers (`render_bindings`), so this is consistent.

- [ ] **Step 4: Build + manual smoke test**

Run: `cargo build`
Expected: PASS (no unit tests for this task — GUI/OS integration).

Manual smoke (record in the milestone, not automated):
1. `cargo run` → Profiles tab. Slots show M1 — Basic / M2 — Media / M3 — (unassigned).
2. Click a library profile → it assigns to the active slot; status "Assigned to M1."; Bindings tab
   reflects it.
3. New → name it → appears in the list. Duplicate → "Copy of …" appears. Rename → display name
   changes, file keeps its name. Delete → confirm modal → file removed; deleting the M1-bound one is
   refused with a message.
4. Change folder… → pick an empty dir → status "Copied N…"; Open folder → Explorer opens the dir.
5. Edit a file externally in the new folder → the app hot-reloads (watcher re-point works).

- [ ] **Step 5: Commit**

```bash
git add src/monitor/mod.rs src/main.rs
git commit -m "feat: rebuilt Profiles tab — library, slots, CRUD, folder controls"
```

---

### Task 8: Ship basic + media; drop default/game; update manifest

Replace the shipped profiles and default manifest per the spec.

**Files:**
- Create: `profiles/basic.toml`
- Modify: `profiles/media.toml`
- Delete: `profiles/default.toml`, `profiles/game.toml`
- Modify: `config.toml`

**Interfaces:** none (content + manifest only).

- [ ] **Step 1: Create `profiles/basic.toml`**

Copy the current `profiles/default.toml` content and prepend a `[meta]` table. Full file:

```toml
# Basic profile — general shortcuts.
# A profile is a full binding set. Keys G1–G22; modifiers: ctrl, shift, alt, windows.

[meta]
name = "Basic"

[keys]
G1  = "ctrl+c"
G2  = "ctrl+v"
G3  = "ctrl+z"
G4  = "ctrl+shift+z"
G5  = "f5"
G6  = "alt+tab"
G7  = "windows+d"
G8  = "ctrl+x"
G9  = "ctrl+s"
G10 = "ctrl+a"
G11 = "ctrl+f"
G12 = "ctrl+w"

[joystick]
mode = "wasd"
deadzone = 30
up = "w"
down = "s"
left = "a"
right = "d"
```

(If the real `profiles/default.toml` has more/other keys, preserve them verbatim — only add the
`[meta]` table and rename the header comment.)

- [ ] **Step 2: Update `profiles/media.toml`**

Prepend the `[meta]` table to the existing file:

```toml
# Media profile — playback / navigation. No joystick section (stick inert).

[meta]
name = "Media"

[keys]
G1 = "space"
G2 = "left"
G3 = "right"
G4 = "up"
G5 = "down"
```

(Preserve any additional keys already in the real file.)

- [ ] **Step 3: Delete the dropped profiles**

```bash
git rm profiles/default.toml profiles/game.toml
```

- [ ] **Step 4: Update `config.toml`**

Set the manifest to the two shipped profiles (keep the existing explanatory comments; change the
mapping lines):

```toml
profiles_dir = "profiles"
m1 = "basic.toml"
m2 = "media.toml"
```

(Remove the `m3 = "media.toml"` line entirely — M3 ships unassigned.)

- [ ] **Step 5: Verify load + tests**

Run: `cargo test`
Expected: PASS — the config-parsing tests use their own temp files, so they're unaffected.

Run: `cargo run` (optional) → app starts; Profiles tab shows M1 — Basic, M2 — Media, M3 —
(unassigned).

- [ ] **Step 6: Commit**

```bash
git add profiles/basic.toml profiles/media.toml config.toml
git commit -m "feat: ship basic + media profiles; drop default/game"
```

---

## Notes for the executor

- Run `cargo test` (never `--lib`). Prepend the toolchain PATH if `cargo`/`gcc` aren't found.
- Tasks 1–6 are pure-logic (TDD). Task 7 is GUI/OS integration (manual-verify, documented
  exception). Task 8 is content.
- After all tasks: final whole-branch review (most capable model), then
  `superpowers:finishing-a-development-branch`. The manual smoke items from Task 7 Step 4 become the
  milestone's smoke-test checklist.
- If `rfd` fails to build under MinGW in Task 1 Step 5, STOP and report — the spec's fallback is a
  path text field instead of the native picker.
