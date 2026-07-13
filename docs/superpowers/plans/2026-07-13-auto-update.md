# In-app auto-update Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The running app checks GitHub Releases for a newer version and, on the user's click, downloads it, verifies its SHA-256, replaces its own exe, and restarts — preserving the user's config/profiles.

**Architecture:** A new `src/update/` module: a platform-agnostic core (`mod.rs` — GitHub check, semver compare, platform-token asset selection, all unit-tested) and an OS-gated apply step (`apply.rs`, `#[cfg(windows)]` — download, verify, extract, self-replace, restart). Runs on a background thread; the GUI Settings tab reads shared `UpdateStatus`.

**Tech Stack:** Rust (GNU toolchain), `ureq` (HTTP+TLS), `serde_json`, `sha2`, `zip`, `self-replace`, `semver`, eframe.

Full design: `docs/superpowers/specs/2026-07-13-auto-update-design.md`.

## Global Constraints

- Build with the **GNU** toolchain; if `cargo`/`gcc` missing: `export PATH="$HOME/.cargo/bin:/c/Strawberry/c/bin:$PATH"`. Binary crate: `cargo test` (NOT `--lib`).
- **TDD** for pure logic (`update/mod.rs` version/asset/parse/select, `apply.rs` sha256 helpers). Network I/O, the zip/self-replace/restart, and the GUI are the documented **manual-verify** exception.
- **No `panic!`/`unwrap()` in the runtime path.** Every update failure logs a warning and leaves the app working on the current version — no crash, no half-update. Checksum mismatch always aborts before install.
- **Platform isolation:** OS-specific code (`apply.rs`) is `#[cfg(windows)]`; no Win32/OS types leak into `update/mod.rs`. Asset selection uses a `PLATFORM` token (`"windows-x64"` today) so multi-OS slots in later.
- **Update = replace `g13-driver.exe` only**; **preserve** `config.toml`/`profiles/`. Auto-restart into the new exe.
- **Version source:** the running app's version is `env!("G13_VERSION")`; releases are tagged `vX.Y.Z` with asset `g13-driver-vX.Y.Z-windows-x64.zip` + `.zip.sha256`.
- **API:** `https://api.github.com/repos/cavefish-dev/g13-driver/releases/latest`, unauthenticated, `User-Agent: g13-driver/<version>`.
- **Pinned deps:** `ureq = "3"`, `serde_json = "1"`, `sha2 = "0.11"`, `zip = { version = "8", default-features = false, features = ["deflate"] }`, `self-replace = "1"`, `semver = "1"`.
- **TLS/crypto build risk (READ):** `ureq` 3 defaults to a rustls TLS stack whose crypto backend (`ring`/`aws-lc-rs`) compiles C — the windows-gnu CI has MinGW gcc so it should build, but this is the highest CI risk. **If the CI build fails on the crypto crate**, switch to Windows SChannel: `ureq = { version = "3", default-features = false, features = ["native-tls"] }` + add `native-tls = "0.2"` (SChannel on Windows, no C crypto to compile). Decide by the real CI run, not on paper.
- API-drift note: for `ureq` 3, `zip` 8, `self-replace` 1, verify the exact API against docs.rs for the pinned version and adapt the code (never the pin).
- One focused commit per task; imperative subject; end each commit message with:
  `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`

---

### Task 1: `update/mod.rs` — platform-agnostic core (version, asset, parse, select)

**Files:**
- Modify: `Cargo.toml` (add `serde_json`, `semver`)
- Create: `src/update/mod.rs`
- Modify: `src/main.rs` (add `mod update;`)

**Interfaces:**
- Produces: `update::PLATFORM: &str`; `asset_name(version: &str) -> String`; `is_newer(latest: &str, current: &str) -> bool`; `Asset`/`ReleaseInfo` (Deserialize); `parse_release(json: &str) -> Result<ReleaseInfo>`; `AvailableUpdate { version, zip_url, sha256_url }`; `select_update(release: &ReleaseInfo, current: &str) -> Option<AvailableUpdate>`; `UpdateStatus`.

- [ ] **Step 1: Add deps**

In `Cargo.toml` `[dependencies]`: `serde_json = "1"` and `semver = "1"`. (serde is already present.)

- [ ] **Step 2: Write the failing tests**

Create `src/update/mod.rs` with tests first:
```rust
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
```

- [ ] **Step 3: Run to verify fail**

Run: `cargo test update::tests`
Expected: FAIL — items not defined.

- [ ] **Step 4: Implement**

Prepend to `src/update/mod.rs`:
```rust
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
```

Add `mod update;` to `src/main.rs` (alphabetical, before `mod usb;`).

- [ ] **Step 5: Run tests to verify pass**

Run: `cargo test` — the 5 new tests pass; all existing pass.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock src/update/mod.rs src/main.rs
git commit -m "feat: auto-update core — version compare + release/asset selection"
```

---

### Task 2: `update/mod.rs` — GitHub fetch (ureq) + `check()`

**Files:**
- Modify: `Cargo.toml` (add `ureq`)
- Modify: `src/update/mod.rs`

**Interfaces:**
- Produces: `fetch_latest_json() -> anyhow::Result<String>`; `check() -> anyhow::Result<Option<AvailableUpdate>>`.
- Consumes: `parse_release`, `select_update` (Task 1).

This is **manual-verify** (network I/O). The pure logic it calls is already tested; there's no new unit test.

- [ ] **Step 1: Add dep**

In `Cargo.toml`: `ureq = "3"`. (See the TLS/crypto build note in Global Constraints — if CI fails on the crypto crate, switch to `default-features = false, features = ["native-tls"]` + `native-tls = "0.2"`.)

- [ ] **Step 2: Implement**

Add to `src/update/mod.rs` (above the `#[cfg(test)]` module):
```rust
const RELEASES_LATEST: &str =
    "https://api.github.com/repos/cavefish-dev/g13-driver/releases/latest";

/// Fetch the latest-release JSON from GitHub (unauthenticated; requires a User-Agent).
pub fn fetch_latest_json() -> anyhow::Result<String> {
    let ua = format!("g13-driver/{}", env!("G13_VERSION"));
    let body = ureq::get(RELEASES_LATEST)
        .header("User-Agent", &ua)
        .header("Accept", "application/vnd.github+json")
        .call()?
        .body_mut()
        .read_to_string()?;
    Ok(body)
}

/// Check GitHub for an update newer than this build. `Ok(None)` = up to date.
pub fn check() -> anyhow::Result<Option<AvailableUpdate>> {
    let json = fetch_latest_json()?;
    let release = parse_release(&json)?;
    Ok(select_update(&release, env!("G13_VERSION")))
}
```
Verify the `ureq` 3 API against docs.rs (`ureq::get(url).header(...).call()?` returning a response whose body is read via `.body_mut().read_to_string()?`, and error types convert into `anyhow` via `?`). Adapt the exact calls if 3.x differs; keep the function signatures.

- [ ] **Step 3: Build + existing suite**

Run: `cargo build && cargo test`
Expected: clean build (watch for the TLS/crypto crate — see the note), all tests pass. (Do NOT hit the network in tests.)

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock src/update/mod.rs
git commit -m "feat: fetch latest release from GitHub + check()"
```

---

### Task 3: `update/apply.rs` — sha256 helpers (TDD) + download/verify/extract

**Files:**
- Modify: `Cargo.toml` (add `sha2`, `zip`)
- Create: `src/update/apply.rs`

**Interfaces:**
- Produces: `sha256_hex(bytes: &[u8]) -> String`; `parse_sha256_file(contents: &str) -> Option<String>`; `install(update: &AvailableUpdate) -> anyhow::Result<()>` (the full apply; self-replace/restart added in Task 4 as `swap_and_restart`).
- Consumes: `crate::update::AvailableUpdate`.

- [ ] **Step 1: Add deps**

In `Cargo.toml`: `sha2 = "0.11"` and `zip = { version = "8", default-features = false, features = ["deflate"] }`.

- [ ] **Step 2: Write the failing tests** (pure helpers)

Create `src/update/apply.rs` with tests first:
```rust
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
```

- [ ] **Step 3: Run to verify fail**

Run: `cargo test apply::tests`
Expected: FAIL — module/functions not found.

- [ ] **Step 4: Implement**

Prepend to `src/update/apply.rs`:
```rust
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
    let mut reader = ureq::get(url)
        .header("User-Agent", concat!("g13-driver/", env!("G13_VERSION")))
        .call()?
        .body_mut()
        .as_reader();
    let mut out = std::fs::File::create(dest)?;
    std::io::copy(&mut reader, &mut out)?;
    out.flush()?;
    Ok(())
}

fn download_to_string(url: &str) -> Result<String> {
    Ok(ureq::get(url)
        .header("User-Agent", concat!("g13-driver/", env!("G13_VERSION")))
        .call()?
        .body_mut()
        .read_to_string()?)
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
```

Also add a temporary stub so this task compiles on its own (Task 4 replaces it):
```rust
fn swap_and_restart(_new_exe: &Path) -> Result<()> {
    bail!("swap_and_restart not implemented yet")
}
```
Verify the `ureq` 3 streaming-body API (`.body_mut().as_reader()` or equivalent) and the `zip` 8 API (`ZipArchive::new`, `.len()`, `.by_index()`, `entry.name()`, `read_to_end`) against docs.rs; adapt calls, keep signatures.

- [ ] **Step 5: Run tests + build**

Run: `cargo test apply::tests` then `cargo build`
Expected: the 2 helper tests pass; clean build (download/extract are untested I/O; the `swap_and_restart` stub keeps it compiling).

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock src/update/apply.rs
git commit -m "feat: update apply — download, verify SHA-256, extract exe"
```

---

### Task 4: Self-replace + restart + `--updated` mutex handoff

**Files:**
- Modify: `src/update/apply.rs` (real `swap_and_restart`)
- Modify: `Cargo.toml` (add `self-replace`)
- Modify: `src/single_instance.rs` (add `acquire_retry`)
- Modify: `src/main.rs` (parse `--updated`, use `acquire_retry` when set)

**Interfaces:**
- Produces: `single_instance::acquire_retry(max_wait: std::time::Duration) -> Acquired`.
- Consumes: `single_instance::{acquire, Acquired}`.

**Manual-verify** (process/OS). Verified in Task 6's smoke test.

- [ ] **Step 1: Add dep**

In `Cargo.toml` under `[target.'cfg(windows)'.dependencies]`: `self-replace = "1"`.

- [ ] **Step 2: `acquire_retry` in `single_instance.rs`**

Add to `src/single_instance.rs`:
```rust
/// Like `acquire`, but if another instance still holds the mutex, retry until it
/// releases (e.g. an updated process waiting for the old one to exit) or `max_wait`
/// elapses. Used on relaunch after a self-update.
pub fn acquire_retry(max_wait: std::time::Duration) -> Acquired {
    let start = std::time::Instant::now();
    loop {
        match acquire() {
            Acquired::First(g) => return Acquired::First(g),
            Acquired::Already => {
                if start.elapsed() >= max_wait {
                    return Acquired::Already;
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        }
    }
}
```

- [ ] **Step 3: Real `swap_and_restart` in `apply.rs`**

Replace the Task 3 `swap_and_restart` stub with:
```rust
/// Replace the running exe with `new_exe`, relaunch (with `--updated`), and exit
/// so the new process can take over the single-instance mutex.
fn swap_and_restart(new_exe: &Path) -> Result<()> {
    self_replace::self_replace(new_exe).context("self-replace failed")?;
    let _ = std::fs::remove_file(new_exe);
    let current = std::env::current_exe().context("current_exe")?;
    std::process::Command::new(current)
        .arg("--updated")
        .spawn()
        .context("relaunch failed")?;
    // Exit the old process; the OS releases the single-instance mutex on exit, and
    // the relaunched `--updated` process retries acquiring it.
    std::process::exit(0);
}
```
Verify the `self-replace` 1.x API (`self_replace::self_replace(path)`) against docs.rs.

- [ ] **Step 4: Handle `--updated` in `main.rs`**

In `src/main.rs`, parse the flag alongside the others:
```rust
    let updated = args.iter().any(|a| a == "--updated");
```
In the Windows GUI branch, use the retry acquire when relaunched after an update:
```rust
        let acq = if updated {
            single_instance::acquire_retry(std::time::Duration::from_secs(10))
        } else {
            single_instance::acquire()
        };
        match acq {
            single_instance::Acquired::Already => {
                single_instance::signal_existing();
                return Ok(());
            }
            single_instance::Acquired::First(guard) => {
                let _guard = guard;
                return monitor::run(config, minimized);
            }
        }
```
(Replace the existing `match single_instance::acquire() { ... }` block with this.)

- [ ] **Step 5: Build + existing suite**

Run: `cargo build && cargo test`
Expected: clean build, all tests pass (no behavior change to tested code).

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock src/update/apply.rs src/single_instance.rs src/main.rs
git commit -m "feat: self-replace + restart with --updated mutex handoff"
```

---

### Task 5: GUI integration — background check + Settings UI

**Files:**
- Modify: `src/monitor/mod.rs`

**Interfaces:**
- Consumes: `crate::update::{UpdateStatus, AvailableUpdate, check}`, `crate::update::apply::install` (Windows).

**Manual-verify** (GUI + threads).

- [ ] **Step 1: Add the shared status + spawn the startup check**

In `MonitorApp` (struct), add a field:
```rust
    update_status: std::sync::Arc<std::sync::Mutex<crate::update::UpdateStatus>>,
```
Initialize it in `MonitorApp::new` to `UpdateStatus::Idle` (wrap in `Arc::new(Mutex::new(...))`), store it on the struct, and — after the tray/consumer setup — spawn the startup check via a shared helper (defined below) with `manual = false`:
```rust
        spawn_update_check(app.update_status.clone(), cc.egui_ctx.clone(), false);
```

Add this free function to `src/monitor/mod.rs`:
```rust
/// Run an update check on a background thread and store the result. A background
/// (manual = false) check that fails goes silently to Idle; a manual check surfaces
/// the error as Failed.
fn spawn_update_check(
    status: std::sync::Arc<std::sync::Mutex<crate::update::UpdateStatus>>,
    ctx: egui::Context,
    manual: bool,
) {
    std::thread::spawn(move || {
        *status.lock().unwrap() = crate::update::UpdateStatus::Checking;
        ctx.request_repaint();
        let next = match crate::update::check() {
            Ok(Some(u)) => crate::update::UpdateStatus::Available(u),
            Ok(None) => crate::update::UpdateStatus::UpToDate,
            Err(e) => {
                log::warn!("update check failed: {e:#}");
                if manual {
                    crate::update::UpdateStatus::Failed("couldn't check for updates".into())
                } else {
                    crate::update::UpdateStatus::Idle
                }
            }
        };
        *status.lock().unwrap() = next;
        ctx.request_repaint();
    });
}
```

- [ ] **Step 2: Settings-tab UI**

`render_settings` takes `&self` and the update status is behind an `Arc<Mutex>`, so it can read + spawn threads via clones. Add an update section at the end of `render_settings` (before the final `ui.weak(...)` help line):
```rust
        ui.add_space(10.0);
        ui.separator();
        ui.label(format!("Version: {}", env!("G13_VERSION")));
        let status = self.update_status.lock().unwrap().clone();
        match status {
            crate::update::UpdateStatus::Checking => { ui.weak("Checking for updates…"); }
            crate::update::UpdateStatus::UpToDate => { ui.weak("Up to date."); }
            crate::update::UpdateStatus::Installing => { ui.weak("Updating… the app will restart."); }
            crate::update::UpdateStatus::Failed(msg) => {
                ui.colored_label(egui::Color32::from_rgb(220, 90, 90), msg);
            }
            crate::update::UpdateStatus::Available(u) => {
                ui.colored_label(egui::Color32::from_rgb(95, 200, 130),
                    format!("Update available: v{}", u.version));
                #[cfg(windows)]
                if ui.button("Update now").clicked() {
                    let status = self.update_status.clone();
                    let upd = u.clone();
                    *status.lock().unwrap() = crate::update::UpdateStatus::Installing;
                    std::thread::spawn(move || {
                        if let Err(e) = crate::update::apply::install(&upd) {
                            log::warn!("update failed: {e:#}");
                            *status.lock().unwrap() =
                                crate::update::UpdateStatus::Failed(format!("update failed: {e:#}"));
                        }
                        // On success install() self-restarts and never returns here.
                    });
                }
            }
            crate::update::UpdateStatus::Idle => {}
        }
        if ui.button("Check for updates").clicked() {
            spawn_update_check(self.update_status.clone(), ui.ctx().clone(), true);
        }
```

- [ ] **Step 3: Build + suite**

Run: `cargo build && cargo test`
Expected: clean build (only the pre-existing `usb.rs` warning), all tests pass. Do NOT launch the GUI here.

- [ ] **Step 4: Commit**

```bash
git add src/monitor/mod.rs
git commit -m "feat: update check on startup + Settings update UI"
```

---

### Task 6: Smoke test + milestone

**Files:**
- Create: `milestones/finished/auto-update.md`

- [ ] **Step 1: Smoke test (needs the user + a newer release)**

Build the current version and confirm the update round-trip against a genuinely newer release:
1. Build + run the CURRENT build (`cargo build --release`; run `target/release/g13-driver.exe`).
2. Cut a newer release: bump BOTH `version.txt` and `Cargo.toml` to the next version (e.g. `0.1.1`) in one commit on `main`, push → CI publishes `v0.1.1`.
3. In the **older** running build's **Settings** tab: it shows the current version and, after the startup check, **"Update available: v0.1.1"**. Click **"Update now"** → it downloads, verifies the SHA-256, swaps the exe, and the app **restarts into v0.1.1** (Settings now shows the new version).
4. Confirm `config.toml`/`profiles/` are unchanged and the persisted Active/Dry-run mode carried over.
5. Confirm a checksum-mismatch or offline case degrades safely (no crash) — e.g. verify the app still runs normally with no network.

- [ ] **Step 2: Write the milestone**

Create `milestones/finished/auto-update.md`:
```markdown
# In-app auto-update

- **Status:** finished
- **Date:** <fill in on completion>

## Outcome
The app checks GitHub Releases on startup and updates itself on the user's click. Spec:
`docs/superpowers/specs/2026-07-13-auto-update-design.md`; plan:
`docs/superpowers/plans/2026-07-13-auto-update.md`. Final MVP sub-project (#3 of 3).
- `update/mod.rs` (platform-agnostic): GitHub latest-release check, semver compare, platform-token
  asset selection (`windows-x64`). `update/apply.rs` (`#[cfg(windows)]`): download, verify SHA-256,
  extract the exe, `self-replace`, relaunch with `--updated` (single-instance retry handoff).
- Replaces the exe only; preserves `config.toml`/`profiles/`; auto-restarts. Checksum verified
  before install; all failures degrade safely (background check fails silently).
- Settings tab: current version, "Check for updates", and "Update now" when newer.
- New deps: ureq, serde_json, sha2, zip, self-replace, semver.

## Follow-ups
- Non-Windows apply (Linux .tar.gz + Unix replace) once those matrix rows exist.
- Fully-automatic updates; tray/toast notifications; refresh LICENSE/README + add-missing-profiles.
- Code-signing (SmartScreen still warns on the new exe); %APPDATA% config location.
```

- [ ] **Step 3: Commit**

```bash
git add milestones/finished/auto-update.md
git commit -m "docs: auto-update milestone"
```

---

## Notes for the executor

- Tasks 1 & 3 have TDD'd pure logic (version/asset/parse/select, sha256 helpers). Tasks 2, 4, 5 are manual-verify (network / process / GUI) with a required docs.rs API check for `ureq`/`zip`/`self-replace`. The real gate is Task 6's smoke test (needs the user + a newer release).
- **Highest risk:** the `ureq` TLS/crypto crate building on the windows-gnu CI — see the Global Constraints note; the controller should watch the first CI build after Task 2 and, if the crypto crate fails, switch to `native-tls` (SChannel) as documented.
- After all tasks: final whole-branch review (most capable model), then `superpowers:finishing-a-development-branch`.
- Cutting `v0.1.1` for the smoke test also bumps the working version to 0.1.1 — that's expected.
