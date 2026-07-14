# Profile Provenance Metadata Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Mark each profile with its origin (`github` vs `user`) and whether a GitHub-sourced profile has been edited, and surface that state in the GUI — groundwork for the future GitHub-download/revert feature.

**Architecture:** Two new optional `[meta]` fields (`source`, `modified`) parsed into a `ProfileSource` enum + `modified: bool` on `Profile`; serialized minimally (omitted for the user/unmodified default). The library CRUD keeps them honest (create/duplicate → `user`, rename preserves), the Bindings-save path flips `modified` for GitHub profiles, `profiles::list` surfaces them, and the Profiles tab shows badges.

**Tech Stack:** Rust (GNU toolchain), eframe/egui 0.31, `serde`/`toml`/`toml_edit`.

## Global Constraints

- GNU toolchain only; if `cargo`/`gcc` missing: `export PATH="$HOME/.cargo/bin:/c/Strawberry/c/bin:$PATH"`. Run `cargo test` (never `--lib` — binary crate).
- No `panic!`/`unwrap()`/`expect()` on profile data. Parsing is defensive: absent/garbage `source` ⇒ `User`; absent `modified` ⇒ `false`.
- **Serialization is minimal:** `to_toml()` writes `source` only when `Github`, `modified` only when `true`. A `user`/unmodified profile serializes with NO `source`/`modified` lines (matches how empty `name` is omitted).
- **Transitions:** New ⇒ `user`. Duplicate ⇒ `user` + `modified=false` (provenance resets). Rename ⇒ `source`/`modified` preserved. Edit-save ⇒ set `modified=true` iff `source==github`. In this task `modified` only ever flips false→true.
- **Bundled `basic`/`media` ship with `source = "github"`.**
- GUI code is manual-verify (documented exception). Branch `feat/profile-provenance` off `main`.
- Commit trailer: `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.

---

### Task 1: Schema — `ProfileSource`, `[meta]` fields, parse & serialize

**Files:**
- Modify: `src/config.rs` — `RawMeta`, new `ProfileSource` enum, `Profile` fields + accessors, `from_raw`, `to_toml`, `Default for Profile`.
- Test: `src/config.rs` (`#[cfg(test)] mod tests`).

**Interfaces:**
- Produces:
  - `pub enum ProfileSource { User, Github }` (Debug, Clone, Copy, PartialEq, Eq, Default=User) with `pub(crate) fn parse(&str) -> Self` and `fn as_str(self) -> Option<&'static str>` (`Github`→`Some("github")`, `User`→`None`).
  - `Profile::source(&self) -> ProfileSource`, `set_source(&mut self, ProfileSource)`, `modified(&self) -> bool`, `set_modified(&mut self, bool)`.

- [ ] **Step 1: Write the failing tests** (in `src/config.rs` `mod tests`)

```rust
#[test]
fn parses_source_and_modified() {
    let src = "[meta]\nname = \"X\"\nsource = \"github\"\nmodified = true\n[keys]\nG1 = \"a\"\n";
    let p = Profile::from_raw(toml::from_str(src).unwrap()).unwrap();
    assert_eq!(p.source(), ProfileSource::Github);
    assert!(p.modified());
}

#[test]
fn source_absent_is_user_and_modified_absent_is_false() {
    let p = Profile::from_raw(raw(&[("G1", "a")])).unwrap();
    assert_eq!(p.source(), ProfileSource::User);
    assert!(!p.modified());
}

#[test]
fn garbage_source_is_user() {
    let src = "[meta]\nsource = \"nonsense\"\n[keys]\nG1 = \"a\"\n";
    let p = Profile::from_raw(toml::from_str(src).unwrap()).unwrap();
    assert_eq!(p.source(), ProfileSource::User);
}

#[test]
fn to_toml_omits_source_and_modified_for_user_default() {
    let p = Profile::from_raw(raw(&[("G1", "a")])).unwrap();
    let toml = p.to_toml().unwrap();
    assert!(!toml.contains("source"));
    assert!(!toml.contains("modified"));
}

#[test]
fn to_toml_round_trips_github_modified() {
    let mut p = Profile::from_raw(raw(&[("G1", "a")])).unwrap();
    p.set_source(ProfileSource::Github);
    p.set_modified(true);
    let toml = p.to_toml().unwrap();
    assert!(toml.contains("source = \"github\""));
    assert!(toml.contains("modified = true"));
    let reloaded = Profile::from_raw(toml::from_str(&toml).unwrap()).unwrap();
    assert_eq!(reloaded.source(), ProfileSource::Github);
    assert!(reloaded.modified());
}

#[test]
fn github_unmodified_omits_modified_line() {
    let mut p = Profile::from_raw(raw(&[("G1", "a")])).unwrap();
    p.set_source(ProfileSource::Github);
    let toml = p.to_toml().unwrap();
    assert!(toml.contains("source = \"github\""));
    assert!(!toml.contains("modified"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test parses_source_and_modified source_absent_is_user garbage_source_is_user to_toml_omits_source to_toml_round_trips_github github_unmodified_omits`
Expected: FAIL — `no method named source` / `cannot find type ProfileSource`.

- [ ] **Step 3: Implement**

Add the `source`/`modified` fields to `RawMeta` (in `src/config.rs`):

```rust
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub(crate) struct RawMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified: Option<bool>,
}
```

Add the enum (near `Profile`):

```rust
/// Where a profile came from. Absent/unknown in the file ⇒ `User`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProfileSource {
    #[default]
    User,
    Github,
}

impl ProfileSource {
    pub(crate) fn parse(s: &str) -> Self {
        if s.trim().eq_ignore_ascii_case("github") { Self::Github } else { Self::User }
    }
    fn as_str(self) -> Option<&'static str> {
        match self { Self::Github => Some("github"), Self::User => None }
    }
}
```

Add fields to `Profile`:

```rust
pub struct Profile {
    key_bindings: HashMap<G13Key, String>,
    joystick: Option<JoystickConfig>,
    repeat: HashMap<G13Key, bool>,
    meta_name: Option<String>,
    source: ProfileSource,
    modified: bool,
}
```

In `from_raw`, replace the `let meta_name = raw.meta … ;` block with a single destructure of all three meta fields, then include them in the returned `Self`:

```rust
        let (meta_name, source, modified) = match raw.meta {
            Some(m) => (
                m.name.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
                m.source.as_deref().map(ProfileSource::parse).unwrap_or_default(),
                m.modified.unwrap_or(false),
            ),
            None => (None, ProfileSource::User, false),
        };

        Ok(Self { key_bindings, joystick, repeat, meta_name, source, modified })
```

Add accessors (near `meta_name`/`set_meta_name`):

```rust
    pub fn source(&self) -> ProfileSource { self.source }
    pub fn set_source(&mut self, source: ProfileSource) { self.source = source; }
    pub fn modified(&self) -> bool { self.modified }
    pub fn set_modified(&mut self, modified: bool) { self.modified = modified; }
```

In `to_toml`, replace the `let meta = self.meta_name.clone().map(…);` line with a builder that includes all three and omits an all-default `[meta]`:

```rust
        let meta = {
            let name = self.meta_name.clone();
            let source = self.source.as_str().map(str::to_string);
            let modified = if self.modified { Some(true) } else { None };
            if name.is_some() || source.is_some() || modified.is_some() {
                Some(RawMeta { name, source, modified })
            } else {
                None
            }
        };
        let raw = RawConfig { meta, keys, joystick, repeat };
```

In `Default for Profile`, add the two fields:

```rust
            meta_name: None,
            source: ProfileSource::User,
            modified: false,
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test`
Expected: PASS — all prior tests + the 6 new ones. (Fix any other `RawMeta { … }` literal the compiler flags — the only one is the `to_toml` builder above.)

- [ ] **Step 5: Commit**

```bash
git add src/config.rs
git commit -m "feat: profile [meta].source/modified schema + ProfileSource"
```

---

### Task 2: Library CRUD honesty + `ProfileEntry` provenance

Make `duplicate` reset provenance, confirm `create` stays user, confirm `rename` preserves, and surface `source`/`modified` through `profiles::list`.

**Files:**
- Modify: `src/profiles.rs` — `ProfileEntry`, `read_display_name` → also read source/modified, `list`, `duplicate`.
- Test: `src/profiles.rs` (`#[cfg(test)] mod tests`).

**Interfaces:**
- Consumes: `crate::config::{ProfileSource, Profile, RawConfig}`.
- Produces: `ProfileEntry { pub filename, pub display_name, pub source: ProfileSource, pub modified: bool }`.

- [ ] **Step 1: Write the failing tests** (in `src/profiles.rs` `mod tests`)

```rust
    #[test]
    fn create_is_user_source() {
        let d = tmp("prov-create");
        let f = create(&d, "Mine").unwrap();
        let p = crate::config::Profile::load(&d.join(&f)).unwrap();
        assert_eq!(p.source(), crate::config::ProfileSource::User);
        assert!(!p.modified());
    }

    #[test]
    fn duplicate_resets_provenance_to_user() {
        let d = tmp("prov-dup");
        std::fs::write(d.join("src.toml"),
            "[meta]\nname = \"Src\"\nsource = \"github\"\nmodified = true\n[keys]\nG1 = \"a\"\n").unwrap();
        let f = duplicate(&d, "src.toml", "Copy").unwrap();
        let p = crate::config::Profile::load(&d.join(&f)).unwrap();
        assert_eq!(p.source(), crate::config::ProfileSource::User);
        assert!(!p.modified());
        assert_eq!(p.get_binding(crate::protocol::G13Key::G1), Some("a")); // bindings copied
    }

    #[test]
    fn rename_preserves_source_and_modified() {
        let d = tmp("prov-rename");
        std::fs::write(d.join("g.toml"),
            "[meta]\nname = \"G\"\nsource = \"github\"\nmodified = true\n[keys]\nG1 = \"a\"\n").unwrap();
        rename(&d, "g.toml", "Renamed").unwrap();
        let p = crate::config::Profile::load(&d.join("g.toml")).unwrap();
        assert_eq!(p.source(), crate::config::ProfileSource::Github);
        assert!(p.modified());
    }

    #[test]
    fn list_surfaces_source_and_modified() {
        let d = tmp("prov-list");
        std::fs::write(d.join("a.toml"),
            "[meta]\nname = \"A\"\nsource = \"github\"\n[keys]\n").unwrap();
        std::fs::write(d.join("b.toml"), "[keys]\nG1 = \"a\"\n").unwrap(); // user
        let entries = list(&d);
        let a = entries.iter().find(|e| e.filename == "a.toml").unwrap();
        let b = entries.iter().find(|e| e.filename == "b.toml").unwrap();
        assert_eq!(a.source, crate::config::ProfileSource::Github);
        assert!(!a.modified);
        assert_eq!(b.source, crate::config::ProfileSource::User);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test create_is_user_source duplicate_resets_provenance rename_preserves_source list_surfaces_source`
Expected: FAIL — `no field source on ProfileEntry` / assertion failures.

- [ ] **Step 3: Implement**

Add `ProfileSource` to the imports at the top of `src/profiles.rs`:

```rust
use crate::config::{ProfileSource, Profile, RawConfig};
```
(Replace the existing separate `use crate::config::RawConfig;` and `use crate::config::Profile;` lines with this single one.)

Extend `ProfileEntry`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileEntry {
    pub filename: String,
    pub display_name: String,
    pub source: ProfileSource,
    pub modified: bool,
}
```

Replace `read_display_name` with a helper that reads all three meta fields leniently:

```rust
/// Lenient read of `[meta]` (name, source, modified) from a profile file.
fn read_entry_meta(path: &Path) -> (Option<String>, ProfileSource, bool) {
    let Ok(text) = std::fs::read_to_string(path) else { return (None, ProfileSource::User, false) };
    let Ok(raw) = toml::from_str::<RawConfig>(&text) else { return (None, ProfileSource::User, false) };
    match raw.meta {
        Some(m) => (
            m.name.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
            m.source.as_deref().map(ProfileSource::parse).unwrap_or_default(),
            m.modified.unwrap_or(false),
        ),
        None => (None, ProfileSource::User, false),
    }
}
```

Update `list` to use it and populate the new fields:

```rust
            let stem = fname.trim_end_matches(".toml").to_string();
            let (name, source, modified) = read_entry_meta(&path);
            let display_name = name.unwrap_or(stem);
            entries.push(ProfileEntry { filename: fname.to_string(), display_name, source, modified });
```

Update `duplicate` to reset provenance after loading the source (add the two `set_` calls before `to_toml`):

```rust
pub fn duplicate(dir: &Path, src_filename: &str, new_display_name: &str) -> Result<String> {
    let mut profile = Profile::load(&dir.join(src_filename))
        .with_context(|| format!("failed to load {src_filename}"))?;
    profile.set_meta_name(Some(new_display_name.to_string()));
    profile.set_source(ProfileSource::User);
    profile.set_modified(false);
    let filename = unique_filename(dir, new_display_name);
    std::fs::write(dir.join(&filename), profile.to_toml()?)
        .with_context(|| format!("failed to write {filename}"))?;
    Ok(filename)
}
```

(`create` needs no change — `Profile::default()` is already `User`/unmodified. `rename` needs no change — it only edits `[meta].name` via `toml_edit`.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test`
Expected: PASS — including the 4 new tests. If a prior `ProfileEntry { … }` literal (in the `list` tests) now fails to compile, it's the `entries.push` above — already updated; no other literals exist.

- [ ] **Step 5: Commit**

```bash
git add src/profiles.rs
git commit -m "feat: provenance in library — duplicate resets, list surfaces source/modified"
```

---

### Task 3: Flip `modified` on edit-save

**Files:**
- Modify: `src/config.rs` — `ProfileSet::save_active_bindings`.
- Test: `src/config.rs` (`#[cfg(test)] mod profileset_tests`).

**Interfaces:**
- Consumes: `Profile::source()`, `Profile::set_modified()`, `ProfileSource::Github` (Task 1).

- [ ] **Step 1: Write the failing tests** (in `mod profileset_tests`)

```rust
    #[test]
    fn saving_github_profile_marks_modified() {
        let d = tmp("flip-github");
        write(&d.join("profiles"), "g.toml",
            "[meta]\nname = \"G\"\nsource = \"github\"\n[keys]\nG1 = \"a\"\n");
        write(&d, "config.toml", "profiles_dir = \"profiles\"\nm1 = \"g.toml\"\n");
        let mut set = ProfileSet::load(&d.join("config.toml")).unwrap();
        let mut b = HashMap::new();
        b.insert(G13Key::G1, "ctrl+c".to_string());
        set.save_active_bindings(b, HashMap::new()).unwrap();
        let text = std::fs::read_to_string(d.join("profiles/g.toml")).unwrap();
        assert!(text.contains("modified = true"), "github profile flips modified on save");
        assert!(text.contains("source = \"github\""), "source preserved");
    }

    #[test]
    fn saving_user_profile_stays_clean() {
        let d = tmp("flip-user");
        write(&d.join("profiles"), "u.toml", "[keys]\nG1 = \"a\"\n");
        write(&d, "config.toml", "profiles_dir = \"profiles\"\nm1 = \"u.toml\"\n");
        let mut set = ProfileSet::load(&d.join("config.toml")).unwrap();
        let mut b = HashMap::new();
        b.insert(G13Key::G1, "ctrl+c".to_string());
        set.save_active_bindings(b, HashMap::new()).unwrap();
        let text = std::fs::read_to_string(d.join("profiles/u.toml")).unwrap();
        assert!(!text.contains("modified"), "user profile stays clean");
        assert!(!text.contains("source"), "user profile stays clean");
    }
```

(`G13Key` is already imported in `mod profileset_tests`; if not, add `use crate::protocol::G13Key;`.)

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test saving_github_profile_marks_modified saving_user_profile_stays_clean`
Expected: FAIL — the github file won't contain `modified = true`.

- [ ] **Step 3: Implement**

In `save_active_bindings`, after `profile.set_repeat(repeat);` and before `let toml = profile.to_toml()?;`, add the flip:

```rust
        profile.set_bindings(bindings);
        profile.set_repeat(repeat);
        if profile.source() == ProfileSource::Github {
            profile.set_modified(true);
        }
        let toml = profile.to_toml()?;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test`
Expected: PASS — including the 2 new tests.

- [ ] **Step 5: Commit**

```bash
git add src/config.rs
git commit -m "feat: flip modified=true when saving a GitHub-sourced profile"
```

---

### Task 4: GUI badges + Bindings note

Show provenance in the Profiles library list and a note on the Bindings tab. GUI — **manual-verify, no unit tests** (documented exception).

**Files:**
- Modify: `src/monitor/mod.rs` — the library-list row in `render_profiles`; `render_bindings`.

**Interfaces:**
- Consumes: `ProfileEntry.source`/`.modified` (Task 2), `Profile::source()`/`modified()` (Task 1), `crate::config::ProfileSource`.

- [ ] **Step 1: Add the library-row badge**

Ensure `ProfileSource` is in scope in `src/monitor/mod.rs` (add `use crate::config::ProfileSource;` near the other `use crate::config::…` imports if not already present).

In `render_profiles`, in the library-list row (right after the clickable display-name `selectable_label`, before the trailing right-to-left button block), add a badge:

```rust
                    match (e.source, e.modified) {
                        (ProfileSource::Github, false) => { ui.weak("GitHub"); }
                        (ProfileSource::Github, true)  => { ui.weak("GitHub · edited"); }
                        (ProfileSource::User, _) => {}
                    }
```

- [ ] **Step 2: Add the Bindings note**

In `render_bindings`, after the guard that obtains `Some(profile)` (the active profile) and before/near the heading area where the profile name is shown, add a source note using the profile's provenance. Capture the flags while the read lock is held (or from the already-held `profile`), e.g. right where the profile is in scope:

```rust
        if profile.source() == ProfileSource::Github {
            if profile.modified() {
                ui.weak("From GitHub · edited — your changes differ from the downloaded version.");
            } else {
                ui.weak("From GitHub — your edits will mark this profile as edited.");
            }
        }
```

(Place this so it renders once, under the "Editing profile: <name>" label. Keep it within the existing lock scope for `profile`, or read the two `Copy`/`bool` values into locals before dropping the guard — do not hold the `profiles` lock across other work.)

- [ ] **Step 3: Build + manual smoke**

Run: `cargo build` (must compile; only the pre-existing `usb.rs` warning) then `cargo test` (unchanged count — no new tests here).

Manual (record for the milestone, not automated): the two bundled profiles show a **GitHub** badge; editing one via the Bindings tab and saving turns it into **GitHub · edited**; a user-created profile shows no badge; the Bindings tab shows the "From GitHub…" note for a GitHub profile and nothing for a user profile.

- [ ] **Step 4: Commit**

```bash
git add src/monitor/mod.rs
git commit -m "feat: Profiles tab shows GitHub / edited provenance badges + Bindings note"
```

---

### Task 5: Mark bundled profiles as GitHub

**Files:**
- Modify: `profiles/basic.toml`, `profiles/media.toml`.

- [ ] **Step 1: Add `source = "github"` to `profiles/basic.toml`**

In its `[meta]` table, add the `source` line (no `modified` — it ships unmodified):

```toml
[meta]
name = "Basic"
source = "github"
```

- [ ] **Step 2: Add `source = "github"` to `profiles/media.toml`**

```toml
[meta]
name = "Media"
source = "github"
```

- [ ] **Step 3: Verify load + tests**

Run: `cargo test` (config-parsing tests use their own temp files — unaffected; must still pass).
Optionally `cargo run` → the Profiles tab shows **GitHub** badges on Basic and Media.

- [ ] **Step 4: Commit**

```bash
git add profiles/basic.toml profiles/media.toml
git commit -m "feat: ship bundled basic/media profiles as source = github"
```

---

## Notes for the executor

- `cargo test` (never `--lib`). Tasks 1–3 and 5 are unit-tested/content; Task 4 is GUI manual-verify.
- After all tasks: final whole-branch review, then `superpowers:finishing-a-development-branch`. The Task 4 manual items become the milestone smoke-test checklist.
