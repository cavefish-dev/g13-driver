# Profile Catalog (download / revert) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let users browse a curated GitHub catalog of profiles, download them into their local library, and revert an edited GitHub-sourced profile to its upstream version.

**Architecture:** A new platform-agnostic `src/catalog.rs` fetches a CI-generated `catalog/index.json` from `raw.githubusercontent.com`, downloads catalog profiles (stamping `source`/`origin`/`modified`), and reverts by re-fetching. An `[meta].origin` schema field records the catalog filename. A new **Catalog** tab browses/downloads (dedup by `origin` → "Downloaded" status); a **Revert** button on the Bindings tab restores upstream. A GitHub Action regenerates the index when `catalog/*.toml` change.

**Tech Stack:** Rust (GNU toolchain), eframe/egui 0.31, `ureq`, `serde_json`, `serde`, `toml`, `anyhow`. No new crates.

## Global Constraints

- GNU toolchain only; if `cargo`/`gcc` missing: `export PATH="$HOME/.cargo/bin:/c/Strawberry/c/bin:$PATH"`. Run `cargo test` (never `--lib`).
- No `panic!`/`unwrap()`/`expect()` on catalog/profile data or in the UI path (lock `.unwrap()` poison idiom excepted; tests may use `unwrap`).
- **`src/catalog.rs` is platform-agnostic** — no `#[cfg]` gating (it's `ureq` + file writes + TOML, works on every arch/OS).
- **Hardcoded base:** repo `cavefish-dev/g13-driver`, branch `main`, catalog dir `catalog`. URLs: `https://raw.githubusercontent.com/cavefish-dev/g13-driver/main/catalog/{index.json | <filename>}`. `User-Agent: g13-driver/<version>` header on every request (GitHub requires a UA).
- **Integrity = "it parses":** a downloaded profile is accepted only if it parses as a valid `Profile` (`toml` + `Profile::from_raw`); no SHA-256 (it's data, not a binary, and there's no per-file checksum).
- **`[meta].origin`** = the catalog filename; present only for `source = github` profiles; stamped by the app on download/revert. **Duplicate clears `origin`; rename preserves it.**
- **Revert restores the whole upstream profile** (bindings + name), re-stamping `source`/`origin`/`modified = false`.
- **Dedup by `origin`:** a catalog entry is "Downloaded" when a local profile carries that `origin`.
- **Threading:** network work runs on a background thread; `catalog_state: Arc<Mutex<CatalogState>>` mirrors the existing `update_status`. Never hold a `profiles` lock across an egui closure or `reload_now`. Browsing is user-initiated (Refresh button; no auto-fetch on launch).
- Branch `feat/profile-catalog` off `main`. Commit trailer: `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.

---

### Task 1: `[meta].origin` schema

**Files:**
- Modify: `src/config.rs` — `RawMeta`, `Profile` field + accessors, `from_raw`, `to_toml`, `Default`.
- Test: `src/config.rs` (`#[cfg(test)] mod tests`).

**Interfaces:**
- Produces: `Profile::origin(&self) -> Option<&str>`, `Profile::set_origin(&mut self, Option<String>)`; `to_toml` emits `[meta].origin` only when `Some`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn parses_origin() {
    let src = "[meta]\nname = \"G\"\nsource = \"github\"\norigin = \"gaming.toml\"\n[keys]\nG1 = \"a\"\n";
    let p = Profile::from_raw(toml::from_str(src).unwrap()).unwrap();
    assert_eq!(p.origin(), Some("gaming.toml"));
}

#[test]
fn origin_absent_is_none() {
    let p = Profile::from_raw(raw(&[("G1", "a")])).unwrap();
    assert_eq!(p.origin(), None);
}

#[test]
fn to_toml_round_trips_origin() {
    let mut p = Profile::from_raw(raw(&[("G1", "a")])).unwrap();
    p.set_source(ProfileSource::Github);
    p.set_origin(Some("gaming.toml".to_string()));
    let toml = p.to_toml().unwrap();
    assert!(toml.contains("origin = \"gaming.toml\""));
    let reloaded = Profile::from_raw(toml::from_str(&toml).unwrap()).unwrap();
    assert_eq!(reloaded.origin(), Some("gaming.toml"));
}

#[test]
fn user_profile_omits_origin() {
    let p = Profile::from_raw(raw(&[("G1", "a")])).unwrap();
    let toml = p.to_toml().unwrap();
    assert!(!toml.contains("origin"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test parses_origin origin_absent_is_none to_toml_round_trips_origin user_profile_omits_origin`
Expected: FAIL — `no method named origin`.

- [ ] **Step 3: Implement**

Add `origin` to `RawMeta` (after `modified`):

```rust
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
```

Add the field to `Profile` (after `modified: bool`):

```rust
    origin: Option<String>,
```

In `from_raw`, extend the meta destructure to a 4-tuple (the `Some(m)` arm and the `None` arm):

```rust
        let (meta_name, source, modified, origin) = match raw.meta {
            Some(m) => (
                m.name.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
                m.source.as_deref().map(ProfileSource::parse).unwrap_or_default(),
                m.modified.unwrap_or(false),
                m.origin.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
            ),
            None => (None, ProfileSource::User, false, None),
        };
```

Add `origin` to the returned `Self { … }` in `from_raw` (find `Ok(Self { key_bindings, joystick, repeat, meta_name, source, modified })` and add `, origin`).

Add accessors (near `source`/`set_source`):

```rust
    pub fn origin(&self) -> Option<&str> { self.origin.as_deref() }
    pub fn set_origin(&mut self, origin: Option<String>) {
        self.origin = origin.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    }
```

In `to_toml`, extend the meta builder to include `origin`:

```rust
        let meta = {
            let name = self.meta_name.clone();
            let source = self.source.as_str().map(str::to_string);
            let modified = if self.modified { Some(true) } else { None };
            let origin = self.origin.clone();
            if name.is_some() || source.is_some() || modified.is_some() || origin.is_some() {
                Some(RawMeta { name, source, modified, origin })
            } else {
                None
            }
        };
```

In `Default for Profile`, add `origin: None,`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test`
Expected: PASS — all prior + the 4 new. (Fix any other `RawMeta { … }` literal the compiler flags — only the `to_toml` builder — by adding `origin`.)

- [ ] **Step 5: Commit**

```bash
git add src/config.rs
git commit -m "feat: add [meta].origin schema field (catalog upstream identity)"
```

---

### Task 2: Profiles library carries `origin`

**Files:**
- Modify: `src/profiles.rs` — `ProfileEntry`, `read_entry_meta`, `list`, `duplicate`.
- Test: `src/profiles.rs` (`#[cfg(test)] mod tests`).

**Interfaces:**
- Consumes: `Profile::set_origin` (Task 1).
- Produces: `ProfileEntry { filename, display_name, source, modified, origin: Option<String> }`.

- [ ] **Step 1: Write the failing tests**

```rust
    #[test]
    fn duplicate_clears_origin() {
        let d = tmp("cat-dup-origin");
        std::fs::write(d.join("src.toml"),
            "[meta]\nname = \"S\"\nsource = \"github\"\norigin = \"s.toml\"\nmodified = true\n[keys]\nG1 = \"a\"\n").unwrap();
        let f = duplicate(&d, "src.toml", "Copy").unwrap();
        let p = crate::config::Profile::load(&d.join(&f)).unwrap();
        assert_eq!(p.origin(), None);
        assert_eq!(p.source(), crate::config::ProfileSource::User);
    }

    #[test]
    fn list_surfaces_origin() {
        let d = tmp("cat-list-origin");
        std::fs::write(d.join("g.toml"),
            "[meta]\nname = \"G\"\nsource = \"github\"\norigin = \"gaming.toml\"\n[keys]\n").unwrap();
        std::fs::write(d.join("u.toml"), "[keys]\nG1 = \"a\"\n").unwrap();
        let entries = list(&d);
        let g = entries.iter().find(|e| e.filename == "g.toml").unwrap();
        let u = entries.iter().find(|e| e.filename == "u.toml").unwrap();
        assert_eq!(g.origin.as_deref(), Some("gaming.toml"));
        assert_eq!(u.origin, None);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test duplicate_clears_origin list_surfaces_origin`
Expected: FAIL — `no field origin on ProfileEntry`.

- [ ] **Step 3: Implement**

Extend `ProfileEntry`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileEntry {
    pub filename: String,
    pub display_name: String,
    pub source: ProfileSource,
    pub modified: bool,
    pub origin: Option<String>,
}
```

Change `read_entry_meta` to return the origin too (a 4-tuple):

```rust
fn read_entry_meta(path: &Path) -> (Option<String>, ProfileSource, bool, Option<String>) {
    let Ok(text) = std::fs::read_to_string(path) else { return (None, ProfileSource::User, false, None) };
    let Ok(raw) = toml::from_str::<RawConfig>(&text) else { return (None, ProfileSource::User, false, None) };
    match raw.meta {
        Some(m) => (
            m.name.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
            m.source.as_deref().map(ProfileSource::parse).unwrap_or_default(),
            m.modified.unwrap_or(false),
            m.origin.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
        ),
        None => (None, ProfileSource::User, false, None),
    }
}
```

Update `list` to destructure and populate:

```rust
            let (name, source, modified, origin) = read_entry_meta(&path);
            let display_name = name.unwrap_or(stem);
            entries.push(ProfileEntry { filename: fname.to_string(), display_name, source, modified, origin });
```

In `duplicate`, clear origin (after the existing `set_source`/`set_modified`):

```rust
    profile.set_source(ProfileSource::User);
    profile.set_modified(false);
    profile.set_origin(None);
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test`
Expected: PASS — including the 2 new tests.

- [ ] **Step 5: Commit**

```bash
git add src/profiles.rs
git commit -m "feat: ProfileEntry carries origin; duplicate clears it"
```

---

### Task 3: `catalog.rs` pure core

**Files:**
- Create: `src/catalog.rs`
- Modify: `src/main.rs` (add `mod catalog;`)
- Test: `src/catalog.rs` (`#[cfg(test)] mod tests`).

**Interfaces:**
- Consumes: `crate::config::{Profile, ProfileSource}`.
- Produces: `CatalogEntry { filename, name }`; `parse_index`, `index_url`, `profile_url`, `mark_downloaded`, `stamp_download`, `parse_profile`; `enum CatalogState`.

- [ ] **Step 1: Write the failing tests**

Create `src/catalog.rs` with the tests first:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test parses_index urls_are_raw_github mark_downloaded_joins stamp_download_sets parse_profile_accepts`
Expected: FAIL — module/functions don't exist.

- [ ] **Step 3: Implement + register the module**

In `src/main.rs`, add alongside the other `mod` lines:

```rust
mod catalog;
```

Add to `src/catalog.rs` (above the test module):

```rust
use std::collections::HashSet;
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test`
Expected: PASS — including the 7 new tests. (`Profile::from_raw` and `meta_name`/`origin` are `pub(crate)`/`pub` — reachable from `catalog.rs`.)

- [ ] **Step 5: Commit**

```bash
git add src/catalog.rs src/main.rs
git commit -m "feat: catalog core — index parse, urls, mark_downloaded, stamp, validate"
```

---

### Task 4: `catalog.rs` network + file ops

Thin network wrappers — **no unit tests** (network; documented exception, same as auto-update's `fetch_latest_json`). Gate: `cargo build` + `cargo test` unchanged.

**Files:**
- Modify: `src/catalog.rs`

**Interfaces:**
- Consumes: `parse_index`, `parse_profile`, `stamp_download`, `profile_url`, `index_url` (Task 3); `crate::profiles::unique_filename`.
- Produces: `fetch_index() -> Result<Vec<CatalogEntry>>`, `download(dir, filename) -> Result<String>`, `revert(path, origin) -> Result<()>`.

- [ ] **Step 1: Implement**

Add to `src/catalog.rs`:

```rust
use std::path::Path;

fn user_agent() -> String { format!("g13-driver/{}", env!("G13_VERSION")) }

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
```

- [ ] **Step 2: Build + test**

Run: `cargo build` then `cargo test`
Expected: `cargo build` clean (only the pre-existing `usb.rs` warning); `cargo test` count unchanged from Task 3 (no new tests). If `ureq`'s `.call()?.body_mut()...` chain triggers a borrow error, bind the response first: `let mut resp = ureq::get(url)...call()?; let body = resp.body_mut().read_to_string()?;`.

- [ ] **Step 3: Commit**

```bash
git add src/catalog.rs
git commit -m "feat: catalog network ops — fetch_index, download, revert"
```

---

### Task 5: Catalog tab UI

New tab that browses + downloads. GUI — **manual-verify, no unit tests**.

**Files:**
- Modify: `src/monitor/mod.rs` — `Tab` enum + `TABS` + render dispatch; `MonitorApp` fields; `spawn_catalog_refresh`; `render_catalog`.

**Interfaces:**
- Consumes: `crate::catalog::{CatalogState, CatalogEntry, fetch_index, download, mark_downloaded}`, `crate::profiles::list`, `crate::runtime::reload_now`.

- [ ] **Step 1: Add the tab**

In the `Tab` enum add `Catalog` (after `Bindings`); bump `TABS` to length 6 and add the entry:

```rust
const TABS: [(Tab, &str); 6] = [
    (Tab::Monitor, "Monitor"),
    (Tab::Profiles, "Profiles"),
    (Tab::Bindings, "Bindings"),
    (Tab::Catalog, "Catalog"),
    (Tab::Lcd, "LCD"),
    (Tab::Settings, "Settings"),
];
```

In the central-panel render dispatch add:

```rust
                Tab::Catalog => self.render_catalog(ui),
```

Add fields to `MonitorApp` (both shared `Arc<Mutex<…>>` so background threads can write them):

```rust
    catalog_state: std::sync::Arc<std::sync::Mutex<crate::catalog::CatalogState>>,
    catalog_status: std::sync::Arc<std::sync::Mutex<Option<String>>>,
```

Initialize in `MonitorApp::new`:

```rust
            catalog_state: std::sync::Arc::new(std::sync::Mutex::new(crate::catalog::CatalogState::Idle)),
            catalog_status: std::sync::Arc::new(std::sync::Mutex::new(None)),
```

Build check: `cargo build` (the `render_catalog` method comes next; add a temporary `fn render_catalog(&mut self, ui: &mut egui::Ui) { let _ = ui; }` if you want an intermediate compile, replaced in Step 2).

- [ ] **Step 2: Refresh thread + render**

Add the background refresh (mirrors `spawn_update_check`) at module scope:

```rust
fn spawn_catalog_refresh(
    state: std::sync::Arc<std::sync::Mutex<crate::catalog::CatalogState>>,
    ctx: egui::Context,
) {
    std::thread::spawn(move || {
        *state.lock().unwrap() = crate::catalog::CatalogState::Loading;
        ctx.request_repaint();
        let next = match crate::catalog::fetch_index() {
            Ok(entries) => crate::catalog::CatalogState::Loaded(entries),
            Err(e) => {
                log::warn!("catalog refresh failed: {e:#}");
                crate::catalog::CatalogState::Failed("couldn't load the catalog".into())
            }
        };
        *state.lock().unwrap() = next;
        ctx.request_repaint();
    });
}
```

Add `render_catalog` to `impl MonitorApp`:

```rust
    fn render_catalog(&mut self, ui: &mut egui::Ui) {
        ui.heading("Catalog");
        ui.label("Download curated profiles from the g13-driver GitHub repo.");
        ui.add_space(6.0);

        if ui.button("Refresh").clicked() {
            spawn_catalog_refresh(self.catalog_state.clone(), ui.ctx().clone());
        }
        ui.add_space(8.0);

        // Snapshot state + local origins under short locks.
        let state = self.catalog_state.lock().unwrap().clone();
        let dir = self.profiles.read().unwrap().profiles_dir().to_path_buf();

        match state {
            crate::catalog::CatalogState::Idle =>
                { ui.weak("Refresh to load the profile catalog from GitHub."); }
            crate::catalog::CatalogState::Loading =>
                { ui.weak("Loading…"); }
            crate::catalog::CatalogState::Failed(msg) =>
                { ui.colored_label(egui::Color32::from_rgb(220,90,90), msg); }
            crate::catalog::CatalogState::Loaded(entries) => {
                let local_origins: std::collections::HashSet<String> = crate::profiles::list(&dir)
                    .into_iter().filter_map(|e| e.origin).collect();
                let marked = crate::catalog::mark_downloaded(entries, &local_origins);
                let mut to_download: Option<String> = None;
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    for (entry, downloaded) in &marked {
                        ui.horizontal(|ui| {
                            ui.label(&entry.name);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if *downloaded {
                                    ui.add_enabled(false, egui::Button::new("Downloaded"));
                                } else if ui.button("Download").clicked() {
                                    to_download = Some(entry.filename.clone());
                                }
                            });
                        });
                    }
                });
                if let Some(filename) = to_download {
                    self.start_download(&dir, &filename, ui.ctx().clone());
                }
            }
        }

        if let Some(s) = self.catalog_status.lock().unwrap().clone() {
            ui.add_space(6.0);
            ui.weak(s);
        }
    }
```

Add the download action to `impl MonitorApp` (runs on a thread; reloads on success — mirrors the "Update now" thread). The `catalog_status` `Arc<Mutex<Option<String>>>` is written by both the spawning frame (immediate feedback) and the worker thread (result), and read by `render_catalog`/`render_bindings` each frame — no draining needed:

```rust
    fn start_download(&mut self, dir: &std::path::Path, filename: &str, ctx: egui::Context) {
        let dir = dir.to_path_buf();
        let filename = filename.to_string();
        let profiles = self.profiles.clone();
        let config_path = self.config_path.clone();
        let status = self.catalog_status.clone();
        *status.lock().unwrap() = Some(format!("Downloading {filename}…"));
        std::thread::spawn(move || {
            let msg = match crate::catalog::download(&dir, &filename)
                .and_then(|local| crate::runtime::reload_now(&profiles, &config_path).map(|_| local)) {
                Ok(local) => format!("Downloaded {local}."),
                Err(e) => { log::warn!("download failed: {e:#}"); format!("Download failed: {e}") }
            };
            *status.lock().unwrap() = Some(msg);
            ctx.request_repaint();
        });
    }
```

**Threading note for the implementer:** `catalog_status` is a shared `Arc<Mutex<Option<String>>>` written by the worker thread and read by the render methods — never hold `self.profiles` (or any other lock) across the `thread::spawn` or an egui closure. `ctx` is passed in from `ui.ctx().clone()` at the call site (no stored ctx handle needed).

- [ ] **Step 3: Build + manual smoke**

Run: `cargo build` then `cargo test` (unchanged count).

Manual (record for the milestone): Catalog tab → Refresh lists the seeded catalog (Gaming, Coding); Download pulls one into the library (Profiles tab shows it with a GitHub badge) and the row flips to "Downloaded"; offline Refresh shows the error label.

- [ ] **Step 4: Commit**

```bash
git add src/monitor/mod.rs
git commit -m "feat: Catalog tab — browse + download profiles from GitHub"
```

---

### Task 6: Revert button (Bindings tab)

GUI — **manual-verify, no unit tests**.

**Files:**
- Modify: `src/monitor/mod.rs` — `render_bindings` (extend the provenance snapshot + add the button + confirm), a `pending_revert` field.

**Interfaces:**
- Consumes: `crate::catalog::revert`, `ProfileSet::active_path`, `crate::runtime::reload_now`.

- [ ] **Step 1: Extend the provenance snapshot**

In `render_bindings`, replace the existing `(is_github, is_modified)` snapshot block so it also captures the active profile's `origin` and file path:

```rust
        let (is_github, is_modified, origin, active_path) = {
            let set = self.profiles.read().unwrap();
            match set.active_profile() {
                Some(profile) => (
                    profile.source() == ProfileSource::Github,
                    profile.modified(),
                    profile.origin().map(String::from),
                    Some(set.active_path()),
                ),
                None => (false, false, None, None),
            }
        };
```

- [ ] **Step 2: Add the Revert button + confirm**

Add a field to `MonitorApp`: `pending_revert: Option<(std::path::PathBuf, String)>` (path + origin), init `None`.

After the existing "From GitHub…" note block in `render_bindings`, add the button (only when github + modified + has origin + has path):

```rust
        if is_github && is_modified {
            if let (Some(origin), Some(path)) = (origin.clone(), active_path.clone()) {
                if ui.button("Revert to GitHub version").clicked() {
                    self.pending_revert = Some((path, origin));
                }
            }
        }
```

Add the confirm + action, rendered near the end of `render_bindings` (mirrors `render_delete_confirm`'s modal + the download thread):

```rust
        if let Some((path, origin)) = self.pending_revert.clone() {
            let mut go = false;
            let mut cancel = false;
            egui::Modal::new(egui::Id::new("revert_confirm")).show(ui.ctx(), |ui| {
                ui.set_width(320.0);
                ui.heading("Revert to GitHub version");
                ui.label("Discard your changes and restore the downloaded version?");
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Revert").clicked() { go = true; }
                    if ui.button("Cancel").clicked() { cancel = true; }
                });
            });
            if go {
                let profiles = self.profiles.clone();
                let config_path = self.config_path.clone();
                let status = self.catalog_status.clone();
                let ctx = ui.ctx().clone();
                *status.lock().unwrap() = Some("Reverting…".to_string());
                std::thread::spawn(move || {
                    let msg = match crate::catalog::revert(&path, &origin)
                        .and_then(|_| crate::runtime::reload_now(&profiles, &config_path)) {
                        Ok(()) => "Reverted to the GitHub version.".to_string(),
                        Err(e) => { log::warn!("revert failed: {e:#}"); format!("Revert failed: {e}") }
                    };
                    *status.lock().unwrap() = Some(msg);
                    ctx.request_repaint();
                });
                self.pending_revert = None;
            } else if cancel {
                self.pending_revert = None;
            }
        }
```

Optionally show the shared status on the Bindings tab too (so revert feedback is visible there): near the provenance note, `if let Some(s) = self.catalog_status.lock().unwrap().clone() { ui.weak(s); }`. Do not hold the `profiles` lock across the spawn or the modal closure.

- [ ] **Step 3: Build + manual smoke**

Run: `cargo build` then `cargo test` (unchanged count).

Manual: download a catalog profile → edit + save it (badge → "GitHub · edited", Bindings shows the note) → **Revert to GitHub version** → confirm → the profile's bindings return to upstream and the "edited" badge clears.

- [ ] **Step 4: Commit**

```bash
git add src/monitor/mod.rs
git commit -m "feat: Revert to GitHub version button on the Bindings tab"
```

---

### Task 7: CI index generator + seed `catalog/`

**Files:**
- Create: `.github/workflows/catalog-index.yml`, `catalog/gaming.toml`, `catalog/coding.toml`, `catalog/index.json`.

**Ruleset note (read first):** `main` has a branch ruleset (PR required + verified signatures). A plain `git push` from CI will be rejected. So the workflow commits `index.json` via the **GitHub API** (`actions/github-script` → `createOrUpdateFileContents`), which produces a **verified, signed** bot commit. Even so, the ruleset may need a bypass entry for the GitHub Actions app — **this is a repo-settings prerequisite the maintainer confirms when the workflow first runs** (like other CI tasks, live verification needs the user). The committed initial `catalog/index.json` (below) means the app works day one regardless.

- [ ] **Step 1: Seed `catalog/gaming.toml`**

```toml
# Gaming profile — number row + WASD on the stick.

[meta]
name = "Gaming"

[keys]
G1 = "1"
G2 = "2"
G3 = "3"
G4 = "4"
G5 = "r"
G6 = "f"
G7 = "space"

[joystick]
mode = "wasd"
deadzone = 30
up = "w"
down = "s"
left = "a"
right = "d"
```

- [ ] **Step 2: Seed `catalog/coding.toml`**

```toml
# Coding profile — common editor shortcuts.

[meta]
name = "Coding"

[keys]
G1 = "ctrl+c"
G2 = "ctrl+v"
G3 = "ctrl+shift+k"
G4 = "ctrl+/"
G5 = "f2"
G6 = "f12"
G7 = "ctrl+shift+f"
G8 = "ctrl+b"
G9 = "ctrl+grave"
```

Note: every binding must be a valid combo (keys from the injector's table). If `ctrl+grave` or `ctrl+shift+k` isn't in `build_key_map()`, substitute a valid key (e.g. `ctrl+f`) — verify by loading (Step 4). Keep it simple and valid.

- [ ] **Step 3: Seed `catalog/index.json`** (sorted by filename)

```json
[
  {"filename": "coding.toml", "name": "Coding"},
  {"filename": "gaming.toml", "name": "Gaming"}
]
```

- [ ] **Step 4: Verify the seeds load**

Run (validates both catalog profiles parse as valid `Profile`s, the same gate the app applies):

```bash
export PATH="$HOME/.cargo/bin:/c/Strawberry/c/bin:$PATH"
cargo test
```
Then a quick parse check of both files — write a throwaway assertion or reuse the app: confirm `cargo build` is clean and, if unsure about a binding, temporarily add `Profile::load(Path::new("catalog/gaming.toml")).unwrap()` in a scratch test, run, then remove it. Ensure both files parse (no unknown keys/combos).

- [ ] **Step 5: Create the workflow `.github/workflows/catalog-index.yml`**

```yaml
name: Catalog Index

on:
  push:
    branches: [main]
    paths: ['catalog/**.toml']
  workflow_dispatch: {}

permissions:
  contents: write

concurrency:
  group: catalog-index
  cancel-in-progress: false

jobs:
  regenerate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0 # v7.0.0
      - name: Build index.json from catalog/*.toml
        shell: bash
        run: |
          python3 - <<'PY'
          import json, pathlib, tomllib
          cat = pathlib.Path("catalog")
          entries = []
          for p in sorted(cat.glob("*.toml")):
              if p.name == "index.json":
                  continue
              try:
                  data = tomllib.loads(p.read_text(encoding="utf-8"))
              except Exception:
                  data = {}
              name = (data.get("meta") or {}).get("name") or p.stem
              entries.append({"filename": p.name, "name": name})
          entries.sort(key=lambda e: e["filename"])
          out = cat / "index.json"
          out.write_text(json.dumps(entries, indent=2) + "\n", encoding="utf-8")
          print(out.read_text())
          PY
      - name: Commit index.json if changed (verified bot commit via API)
        uses: actions/github-script@60a0d83039c74a4aee543508d2ffcb1c3799cdc9 # v7.0.1
        with:
          script: |
            const fs = require('fs');
            const path = 'catalog/index.json';
            const content = fs.readFileSync(path, 'utf8');
            const b64 = Buffer.from(content, 'utf8').toString('base64');
            let sha;
            try {
              const { data } = await github.rest.repos.getContent({
                owner: context.repo.owner, repo: context.repo.repo, path, ref: 'main' });
              sha = data.sha;
              const current = Buffer.from(data.content, 'base64').toString('utf8');
              if (current === content) { core.info('index.json unchanged'); return; }
            } catch (e) { core.info('index.json does not exist yet'); }
            await github.rest.repos.createOrUpdateFileContents({
              owner: context.repo.owner, repo: context.repo.repo, path,
              message: 'chore: regenerate catalog index.json',
              content: b64, sha, branch: 'main' });
            core.info('index.json committed');
```

(SHA-pin `github-script` to the current v7 commit; the pin shown is `v7.0.1`.)

- [ ] **Step 6: Commit**

```bash
git add catalog/ .github/workflows/catalog-index.yml
git commit -m "feat: seed catalog + CI index generator"
```

---

## Notes for the executor

- Run `cargo test` (never `--lib`). Tasks 1–3 are unit-tested; Task 4 is network (build-gated); Tasks 5–6 are GUI manual-verify; Task 7 is CI + content.
- After all tasks: final whole-branch review, then `superpowers:finishing-a-development-branch`. The manual smoke items (Tasks 5–6) + the live CI index run (Task 7) become the milestone smoke-test checklist — the CI push + any ruleset-bypass confirmation needs the maintainer.
- When staging a GUI smoke test, sync the beside-exe bundle first: copy the repo `config.toml` + `profiles/` into `target/release/` (the app reads beside-the-exe first).
