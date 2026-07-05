# Bindings Editor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Edit the active profile's G-key bindings in the Bindings tab (text field + hints + live validation) and Save them back to that profile's `.toml` file.

**Architecture:** The config layer gains serialize/persist: `Profile::to_toml` (whole-profile serialize) and `ProfileSet::save_active_bindings` (write the active profile's file). The GUI Bindings tab becomes an editor with per-key edit buffers, `KeyCombo`-based live validation, and Save/Revert. Saving writes the file; the existing watcher hot-reloads it.

**Tech Stack:** Rust, GNU toolchain (`stable-x86_64-pc-windows-gnu`), `eframe`/`egui` 0.31.1, `toml`/`serde`, `log`. Build/test: `cargo` (PATH may need `export PATH="$HOME/.cargo/bin:/c/Strawberry/c/bin:$PATH"`).

## Global Constraints

- **Windows-only** (`src/main.rs:1-2`). OS injection behind `#[cfg(windows)]` in `src/injector/`. `config`/`monitor` stay platform-neutral (no Win32).
- **TDD** for pure/IO logic (`Profile::to_toml` round-trip; `ProfileSet::save_active_bindings`). GUI rendering is manual-verify (documented exception) — verified by the smoke test.
- **Persistence = whole-profile serialize (Approach 1):** rebuild the profile's TOML from memory on Save; comments/formatting are lost when the GUI rewrites a profile file (accepted trade). No new dependency.
- **Validation reuses `crate::injector::KeyCombo::parse`** — "valid in the editor" == "injects at runtime". Empty field = unmapped (key omitted from saved `[keys]`).
- **Save writes the ACTIVE profile file** (`profiles/<active>.toml`; legacy: `config.toml` itself); the manifest and other profile files are untouched; the joystick section is carried through unchanged.
- **Error policy:** write failures `log::warn!` + show an error line in the tab; no `panic!`/`unwrap()` in the runtime path (test code may `unwrap`; `mutex/rwlock.unwrap()` is the accepted poison-unreachable exception).
- **Commits:** one per task; imperative subject; end every message with the trailer `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.
- Binary crate — `cargo test` (not `--lib`); focused: `cargo test <module>::`.

---

## File Structure

| File | Change | Responsibility |
|------|--------|----------------|
| `src/config.rs` | Modify | `Serialize` on raw structs; `Profile::bindings`/`set_bindings`/`to_toml`; `ProfileSet::active_path`/`active_profile_mut`/`save_active_bindings` |
| `src/monitor/mod.rs` | Modify | Bindings tab editor (edit buffers, hints, validation, Save/Revert) |
| milestone doc | Create | `milestones/finished/gui-bindings-editor.md` |

---

## Task 1: Config layer — serialize + persist

**Files:** Modify `src/config.rs`.

**Interfaces:**
- Produces:
  - `Profile::bindings(&self) -> &HashMap<G13Key, String>`
  - `Profile::set_bindings(&mut self, bindings: HashMap<G13Key, String>)`
  - `Profile::to_toml(&self) -> Result<String>`
  - `ProfileSet::active_path(&self) -> PathBuf`
  - `ProfileSet::save_active_bindings(&mut self, bindings: HashMap<G13Key, String>) -> Result<()>`

- [ ] **Step 1: Write the failing tests**

Add to the `profileset_tests` module in `src/config.rs` (it already has the `write`/`tmp` helpers and `use crate::protocol::MKey;`; also add `use crate::protocol::G13Key;` and `use std::collections::HashMap;` to that module):

```rust
    #[test]
    fn profile_to_toml_round_trips() {
        // A profile with keys + joystick serializes and reloads identically.
        let src = "[keys]\nG1 = \"ctrl+c\"\nG5 = \"f5\"\n[joystick]\nmode = \"wasd\"\ndeadzone = 20\nup = \"w\"\n";
        let raw: RawConfig = toml::from_str(src).unwrap();
        let p = Profile::from_raw(raw).unwrap();
        let toml = p.to_toml().unwrap();
        let reloaded = Profile::from_raw(toml::from_str(&toml).unwrap()).unwrap();
        assert_eq!(reloaded.get_binding(G13Key::G1), Some("ctrl+c"));
        assert_eq!(reloaded.get_binding(G13Key::G5), Some("f5"));
        let j = reloaded.joystick().expect("joystick preserved");
        assert_eq!(j.deadzone, 20);
        assert_eq!(j.up.as_deref(), Some("w"));
    }

    #[test]
    fn save_active_bindings_writes_and_preserves_others() {
        let d = tmp("save");
        write(&d.join("profiles"), "default.toml", "[keys]\nG1 = \"ctrl+c\"\n[joystick]\nup = \"w\"\n");
        write(&d.join("profiles"), "game.toml", "[keys]\nG1 = \"space\"\n");
        write(&d, "config.toml", "profiles_dir = \"profiles\"\nm1 = \"default.toml\"\nm2 = \"game.toml\"\n");
        let mut set = ProfileSet::load(&d.join("config.toml")).unwrap();

        // Edit M1 (active): G1 -> ctrl+a, add G2 -> f1.
        let mut b = HashMap::new();
        b.insert(G13Key::G1, "ctrl+a".to_string());
        b.insert(G13Key::G2, "f1".to_string());
        set.save_active_bindings(b).unwrap();

        // Fresh load from disk reflects the change; joystick preserved; game untouched.
        let reloaded = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert_eq!(reloaded.active_profile().get_binding(G13Key::G1), Some("ctrl+a"));
        assert_eq!(reloaded.active_profile().get_binding(G13Key::G2), Some("f1"));
        assert!(reloaded.active_profile().joystick().is_some(), "joystick preserved");
        // M2 file untouched.
        let game = std::fs::read_to_string(d.join("profiles/game.toml")).unwrap();
        assert!(game.contains("space"));
        // Manifest untouched.
        let manifest = std::fs::read_to_string(d.join("config.toml")).unwrap();
        assert!(manifest.contains("m1 = \"default.toml\""));
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test profileset_tests:: 2>&1 | tail -15`
Expected: FAIL — `no method to_toml` / `no method save_active_bindings`.

- [ ] **Step 3: Add `Serialize` to the raw structs**

In `src/config.rs`, change the derives (add `Serialize`) and the import:

```rust
use serde::{Deserialize, Serialize};
```

```rust
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RawConfig {
    #[serde(default)]
    pub keys: HashMap<String, String>,
    #[serde(default)]
    pub joystick: Option<RawJoystick>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RawJoystick {
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(default = "default_deadzone")]
    pub deadzone: u16,
    pub up: Option<String>,
    pub down: Option<String>,
    pub left: Option<String>,
    pub right: Option<String>,
}
```

- [ ] **Step 4: Add the `Profile` methods**

In `src/config.rs`, add to `impl Profile` (after `joystick`):

```rust
    pub fn bindings(&self) -> &HashMap<G13Key, String> {
        &self.key_bindings
    }

    pub fn set_bindings(&mut self, bindings: HashMap<G13Key, String>) {
        self.key_bindings = bindings;
    }

    /// Serialize this profile back to TOML (keys + joystick). Comments in the
    /// original file are not preserved (the file becomes GUI-managed).
    pub fn to_toml(&self) -> Result<String> {
        let keys: HashMap<String, String> = self.key_bindings.iter()
            .map(|(k, v)| (format!("{k:?}"), v.clone())) // Debug of G13Key is "G1".."G22"
            .collect();
        let joystick = self.joystick.as_ref().map(|j| RawJoystick {
            mode: match j.mode {
                JoystickMode::Wasd => "wasd".to_string(),
                JoystickMode::Mouse => "mouse".to_string(),
            },
            deadzone: j.deadzone as u16,
            up: j.up.clone(),
            down: j.down.clone(),
            left: j.left.clone(),
            right: j.right.clone(),
        });
        let raw = RawConfig { keys, joystick };
        toml::to_string(&raw).context("failed to serialize profile")
    }
```

- [ ] **Step 5: Add the `ProfileSet` persistence methods**

In `src/config.rs`, add to `impl ProfileSet` (after `available`):

```rust
    /// The file path backing the active profile (profiles_dir + active filename;
    /// for a legacy single-profile config that resolves to the config file).
    pub fn active_path(&self) -> PathBuf {
        let name = self.active_name().unwrap_or("config.toml");
        self.profiles_dir.join(name)
    }

    fn active_profile_mut(&mut self) -> &mut Profile {
        // Invariant: `active` points at a populated slot (or M1).
        if self.active == MKey::M2 {
            if let Some(p) = self.m2.as_mut() { return p; }
        } else if self.active == MKey::M3 {
            if let Some(p) = self.m3.as_mut() { return p; }
        }
        &mut self.m1
    }

    /// Replace the active profile's key bindings (joystick untouched) and write
    /// the profile file. The watcher will reload the identical content.
    pub fn save_active_bindings(&mut self, bindings: HashMap<G13Key, String>) -> Result<()> {
        let path = self.active_path();
        let profile = self.active_profile_mut();
        profile.set_bindings(bindings);
        let toml = profile.to_toml()?;
        std::fs::write(&path, toml)
            .with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }
```

(`MKey` derives `PartialEq` already, so `self.active == MKey::M2` compiles. `PathBuf` is already imported.)

- [ ] **Step 6: Run to verify pass**

Run: `cargo test profileset_tests:: 2>&1 | tail -8`
Expected: PASS (both new tests + existing).
Run: `cargo test 2>&1 | tail -3` → full suite green.

- [ ] **Step 7: Commit**

```bash
git add src/config.rs
git commit -m "feat: serialize profiles and persist active-profile binding edits

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: GUI — the Bindings editor

**Files:** Modify `src/monitor/mod.rs`.

**Interfaces:**
- Consumes: `Profile::bindings`, `ProfileSet::{active_name, save_active_bindings}` (Task 1); `crate::injector::KeyCombo::parse`.

- [ ] **Step 1: Add edit state to `MonitorApp` + imports**

In `src/monitor/mod.rs`, add imports near the top:

```rust
use std::collections::HashMap;
use crate::injector::KeyCombo;
```

Add fields to the `MonitorApp` struct:

```rust
pub struct MonitorApp {
    profiles: Arc<RwLock<ProfileSet>>,
    state: Arc<Mutex<DeviceState>>,
    dry_run: Arc<AtomicBool>,
    tab: Tab,
    edits: HashMap<G13Key, String>,
    edits_for: Option<String>,
    save_status: Option<String>,
}
```

In `MonitorApp::new`, initialise them in the `Self { ... }` literal:

```rust
        let app = Self {
            profiles,
            state,
            dry_run,
            tab: Tab::Monitor,
            edits: HashMap::new(),
            edits_for: None,
            save_status: None,
        };
```

- [ ] **Step 2: Make `render_bindings` take `&mut self`**

In `src/monitor/mod.rs`, change the `update` dispatch arm for Bindings to keep calling `self.render_bindings(ui)` (no change needed there — `update` already holds `&mut self`), and change the method signature + body. Replace the entire existing `fn render_bindings(&self, ui: &mut egui::Ui) { ... }` with:

```rust
    fn render_bindings(&mut self, ui: &mut egui::Ui) {
        // Which profile are we editing? Reload buffers when it changes.
        let active_name = self.profiles.read().unwrap().active_name().map(String::from);
        if self.edits_for != active_name {
            let set = self.profiles.read().unwrap();
            let profile = set.active_profile();
            self.edits = ROWS.iter().flatten()
                .map(|&k| (k, profile.get_binding(k).unwrap_or("").to_string()))
                .collect();
            drop(set);
            self.edits_for = active_name.clone();
            self.save_status = None;
        }

        ui.heading("Bindings");
        match &active_name {
            Some(n) => ui.label(format!("Editing profile: {n}")),
            None => ui.label("No profile loaded"),
        };
        ui.weak("Combo = optional modifiers (ctrl / shift / alt / win) + one key.  \
                 Keys: a-z, 0-9, f1-f24, enter, esc, space, tab, arrows, home/end, \
                 pageup/pagedown, insert/delete.  Examples: ctrl+c, ctrl+shift+z, win+d.  \
                 Empty = unmapped.");
        ui.add_space(6.0);

        let green = egui::Color32::from_rgb(127, 224, 160);
        let red = egui::Color32::from_rgb(220, 90, 90);
        let dim = egui::Color32::from_gray(110);

        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            for row in ROWS {
                for &key in row {
                    let buf = self.edits.entry(key).or_default();
                    ui.horizontal(|ui| {
                        ui.monospace(format!("{key:?}"));
                        ui.add_space(6.0);
                        ui.add(egui::TextEdit::singleline(buf).desired_width(160.0));
                        // Compute validity AFTER the edit so the mark has no one-frame lag.
                        let (mark, color) = if buf.is_empty() {
                            ("—", dim)
                        } else if KeyCombo::parse(buf).is_ok() {
                            ("ok", green)
                        } else {
                            ("bad", red)
                        };
                        ui.colored_label(color, mark);
                    });
                }
            }
        });

        ui.add_space(8.0);
        let all_valid = self.edits.values().all(|b| b.is_empty() || KeyCombo::parse(b).is_ok());
        ui.horizontal(|ui| {
            if ui.add_enabled(all_valid, egui::Button::new("Save")).clicked() {
                let bindings: HashMap<G13Key, String> = self.edits.iter()
                    .filter(|(_, v)| !v.is_empty())
                    .map(|(k, v)| (*k, v.clone()))
                    .collect();
                match self.profiles.write().unwrap().save_active_bindings(bindings) {
                    Ok(()) => self.save_status = Some("saved".to_string()),
                    Err(e) => {
                        log::warn!("save failed: {e:#}");
                        self.save_status = Some(format!("save failed: {e:#}"));
                    }
                }
            }
            if ui.button("Revert").clicked() {
                self.edits_for = None; // forces a reload from the profile next frame
            }
            if let Some(s) = &self.save_status {
                ui.label(s);
            }
        });
        if !all_valid {
            ui.colored_label(red, "Fix the invalid (bad) combos before saving.");
        }
    }
```

- [ ] **Step 3: Build**

Run: `cargo build 2>&1 | tail -3`
Expected: `Finished`. If a borrow-checker error appears in `update`'s CentralPanel closure (because `render_bindings` is now `&mut self` while the closure also calls `&self` render methods): confirm `snapshot` is cloned into an owned value BEFORE the closure (it is — `let snapshot = self.state.lock().unwrap().clone();`), so the closure can capture `&mut self` and `&snapshot` disjointly. If it still fails, wrap the Bindings arm as `Tab::Bindings => { self.render_bindings(ui); }` — no other change should be needed.

- [ ] **Step 4: Full test suite**

Run: `cargo test 2>&1 | tail -3`
Expected: green (no new unit tests; the editor is manual-verify). Only the pre-existing `usb.rs` warning.

- [ ] **Step 5: Commit**

```bash
git add src/monitor/mod.rs
git commit -m "feat: editable Bindings tab (text + hints + live validation + save)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: Hardware smoke test + docs

**Files:** Create `milestones/finished/gui-bindings-editor.md`; modify `CLAUDE.md` (roadmap note).

- [ ] **Step 1: Build release + full test**

Run: `cargo build --release 2>&1 | tail -2` → `Finished`.
Run: `cargo test 2>&1 | tail -3` → green.

- [ ] **Step 2: Hardware / desktop smoke test (manual)**

```bash
export PATH="$HOME/.cargo/bin:/c/Strawberry/c/bin:$PATH"
export RUST_LOG=info
./target/release/g13-driver.exe
```
On the **Bindings** tab, confirm ALL of:
- Header shows `Editing profile: default.toml`; every G1–G22 field is pre-filled with the current binding; the hints line is visible.
- Type an invalid combo (e.g. `ctrl+`) → the row shows `bad` (red) and **Save is disabled**.
- Fix it (e.g. `ctrl+p`) → row shows `ok`; **Save** persists → `saved`; `profiles/default.toml` on disk now has the new binding, and the log shows `config reloaded`. In Active mode, G-key now injects the new combo.
- Empty a field, Save → that key becomes unmapped (removed from the file).
- **Revert** discards unsaved edits (fields reload from the file).
- Press **M2** (or click M2 on Profiles) → the Bindings tab now edits `game.toml` (fields reload); edit + Save writes `game.toml`, leaving `default.toml` untouched.

- [ ] **Step 3: Milestone + roadmap note**

Create `milestones/finished/gui-bindings-editor.md`:

```markdown
# GUI: Bindings editor

- **Status:** finished
- **Date:** 2026-07-05
- **Part of:** "Finish the GUI" — sub-project 1 (Bindings editing).

## Outcome
Hardware-verified. The Bindings tab edits the active profile's G-key bindings (text field +
syntax hints + live `KeyCombo` validation) and saves them back to the profile `.toml`
(whole-profile serialize; comments not preserved). Spec:
`docs/superpowers/specs/2026-07-05-bindings-editor-design.md`; plan:
`docs/superpowers/plans/2026-07-05-bindings-editor.md`.
- `Profile::to_toml` + `ProfileSet::save_active_bindings` (config layer).
- Editable Bindings tab with Save/Revert; empty = unmapped; invalid combos block Save.
- Saving hot-reloads via the existing watcher.

## Remaining GUI-completion work
- Capture mode (press keys to record a combo).
- Joystick editing (Settings); deadzone slider.
- Profile file management (New/Rename/Delete; reassign files to M1/M2/M3).
- Editing a non-active profile without switching to it (profile-to-edit selector).
- Comment-preserving saves (`toml_edit`).
- LCD tab — deferred to the v0.4 LCD output protocol.
```

In `CLAUDE.md`, under the existing GUI note in the Roadmap section, append:

```markdown
> The GUI's **Bindings** tab now edits the active profile's key bindings and saves them
> back to the profile file. See `milestones/finished/gui-bindings-editor.md`.
```

- [ ] **Step 4: Commit**

```bash
git add milestones/finished/gui-bindings-editor.md CLAUDE.md
git commit -m "docs: record GUI bindings-editor milestone (hardware-verified)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Self-Review

**Spec coverage:**
- `Serialize` on raw structs; `Profile::to_toml` (whole-profile serialize) → Task 1 ✓
- `Profile::bindings`/`set_bindings`; `ProfileSet::active_path`/`save_active_bindings` (write active file, joystick preserved, manifest + others untouched) → Task 1 ✓
- Editor: header (editing profile), hints, editable G1–G22 rows, live `KeyCombo` validation, Save (disabled on invalid) / Revert, empty=unmapped, reload-on-profile-change → Task 2 ✓
- Error handling (write failure → warn + status line, no crash) → Task 2 ✓
- Manifest vs legacy write target (via `active_path`) → Task 1 ✓
- Testing (to_toml round-trip; save writes/preserves; GUI manual) → Tasks 1,3 ✓
- Milestone/roadmap → Task 3 ✓

**Deviations:** none. Implementation detail: `to_toml` derives the `"G1"` key string from `format!("{k:?}")` (the inverse of `parse_g13_key`, guarded by the round-trip test); `[keys]` serialize order is unspecified (HashMap) — cosmetic only.

**Placeholder scan:** none — every step has concrete code/commands.

**Type consistency:** `Profile::{bindings,set_bindings,to_toml}`, `ProfileSet::{active_path,active_profile_mut,save_active_bindings}`, `MonitorApp.{edits,edits_for,save_status}`, `KeyCombo::parse` — used consistently across tasks.
