# Monitor Layout & Programmable Joystick Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Monitor mirror the physical G13 (M-keys on top, thumb cells beside the joystick, real joystick bindings) and make the joystick programmable from the GUI, with a single global deadzone on the Settings tab.

**Architecture:** `JoystickConfig` is simplified to the four directions (mode + per-profile deadzone removed); deadzone becomes a global value on `ProfileSet` (manifest `[joystick] deadzone`, edited on Settings). Consumers (dispatcher, `joystick.rs`, Monitor) read the global deadzone; `save_active_bindings` grows a joystick arg for the Bindings direction editor; `render_monitor` is rearranged.

**Tech Stack:** Rust (GNU toolchain), eframe/egui 0.31, `serde`/`toml`/`toml_edit`.

## Global Constraints

- GNU toolchain only; if `cargo`/`gcc` missing: `export PATH="$HOME/.cargo/bin:/c/Strawberry/c/bin:$PATH"`. Run `cargo test` (never `--lib`).
- **No mouse mode** — WASD (stick direction → key combo) is the only behavior; `mode` is removed from the model. `RawJoystick` still *parses* `mode`/`deadzone` (ignored) so legacy profiles load; they drop out on the next save.
- **Deadzone is global** — a manifest `[joystick] deadzone = <0..=127>` (default 30, clamp >127 → keep ≤127), on `ProfileSet`, edited on the Settings tab. Per-profile `[joystick]` = the four directions only.
- **Directions validated like key combos** (`combo_valid`); an empty direction = unmapped; all-empty ⇒ no `[joystick]` written.
- Consumers read the global deadzone; `handle_joystick` no longer filters on mode. Editing a direction flips `modified` on a GitHub profile (via `save_active_bindings`).
- No `panic!`/`unwrap()`/`expect()` on profile data (lock-poison `.unwrap()` excepted). GUI code is manual-verify (documented exception).
- Branch `feat/monitor-joystick-ux` off `main`. Commit trailer: `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.

---

### Task 1: Global deadzone on `ProfileSet`

Additive — a global deadzone from the manifest `[joystick]` table, with getter/setter/persist. No consumer changes yet (Task 2 rewires them).

**Files:**
- Modify: `src/config.rs` — `RawManifest`, a new `RawManifestJoystick`, `ProfileSet` field + both `load` branches, accessor, setter, `persist_joystick_deadzone`.
- Test: `src/config.rs` (`mod profileset_tests`).

**Interfaces:**
- Produces: `ProfileSet::joystick_deadzone(&self) -> u8`, `ProfileSet::set_joystick_deadzone(&mut self, u8)`, `ProfileSet::persist_joystick_deadzone(&self, u8) -> Result<()>`.

- [ ] **Step 1: Write the failing tests**

```rust
    #[test]
    fn global_deadzone_defaults_to_30() {
        let d = tmp("gdz-default");
        write(&d, "config.toml", "[keys]\nG1 = \"a\"\n");
        let set = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert_eq!(set.joystick_deadzone(), 30);
    }

    #[test]
    fn global_deadzone_parses_and_clamps() {
        let d = tmp("gdz-parse");
        write(&d.join("profiles"), "p.toml", "[keys]\nG1 = \"a\"\n");
        write(&d, "config.toml",
            "profiles_dir = \"profiles\"\nm1 = \"p.toml\"\n[joystick]\ndeadzone = 200\n");
        let set = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert_eq!(set.joystick_deadzone(), 127); // clamped
    }

    #[test]
    fn persist_joystick_deadzone_writes_and_reloads() {
        let d = tmp("gdz-persist");
        write(&d.join("profiles"), "p.toml", "[keys]\nG1 = \"a\"\n");
        write(&d, "config.toml", "# manifest\nprofiles_dir = \"profiles\"\nm1 = \"p.toml\"\n");
        let set = ProfileSet::load(&d.join("config.toml")).unwrap();
        set.persist_joystick_deadzone(42).unwrap();
        let reloaded = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert_eq!(reloaded.joystick_deadzone(), 42);
        let text = std::fs::read_to_string(d.join("config.toml")).unwrap();
        assert!(text.contains("# manifest"), "comment preserved");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test global_deadzone_defaults global_deadzone_parses persist_joystick_deadzone`
Expected: FAIL — `no method named joystick_deadzone`.

- [ ] **Step 3: Implement**

Add the manifest joystick struct + default (near `RawAutoRepeat`):

```rust
#[derive(Debug, Deserialize)]
struct RawManifestJoystick {
    #[serde(default = "default_global_deadzone")]
    deadzone: u16,
}
fn default_global_deadzone() -> u16 { 30 }
```

Add the field to `RawManifest`:

```rust
    #[serde(default)]
    joystick: Option<RawManifestJoystick>,
```

Add the field to `ProfileSet` (near `autorepeat`):

```rust
    joystick_deadzone: u8,
```

In `load`, compute it once (near `let autorepeat = …`) and add it to BOTH the manifest-mode and legacy-mode `Self { … }` constructors:

```rust
        let joystick_deadzone = raw.joystick.map(|j| j.deadzone.min(127) as u8).unwrap_or(30);
```

Add accessor/setter/persist (near `autorepeat()` / `persist_start_active`):

```rust
    pub fn joystick_deadzone(&self) -> u8 { self.joystick_deadzone }

    pub fn set_joystick_deadzone(&mut self, deadzone: u8) {
        self.joystick_deadzone = deadzone.min(127);
    }

    pub fn persist_joystick_deadzone(&self, deadzone: u8) -> Result<()> {
        use toml_edit::{DocumentMut, Item, Table, value as toml_value};
        let text = std::fs::read_to_string(&self.config_path)
            .with_context(|| format!("failed to read {}", self.config_path.display()))?;
        let mut doc = text.parse::<DocumentMut>()
            .with_context(|| format!("failed to parse {}", self.config_path.display()))?;
        if !doc.as_table().contains_key("joystick") {
            doc.as_table_mut().insert("joystick", Item::Table(Table::new()));
        }
        doc["joystick"]["deadzone"] = toml_value(deadzone.min(127) as i64);
        std::fs::write(&self.config_path, doc.to_string())
            .with_context(|| format!("failed to write {}", self.config_path.display()))?;
        Ok(())
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test`
Expected: PASS — all prior + the 3 new. (Add `joystick_deadzone` to any `ProfileSet { … }` literal the compiler flags — both `load` branches.)

- [ ] **Step 5: Commit**

```bash
git add src/config.rs
git commit -m "feat: global joystick deadzone on ProfileSet (manifest [joystick])"
```

---

### Task 2: Simplify `JoystickConfig` + consumer ripple (atomic)

Remove `mode` and per-profile `deadzone` from `JoystickConfig`; rewire every consumer to the global deadzone. This is ONE compile unit — the type change ripples across `config.rs`, `joystick.rs`, `dispatcher.rs`, `runtime.rs`, and the Monitor joystick panel.

**Files:**
- Modify: `src/config.rs` — `RawJoystick`, remove `JoystickMode`, `JoystickConfig`, `parse_joystick`, `to_toml` joystick block, add `Profile::set_joystick`.
- Modify: `src/joystick.rs` — `update` takes a `deadzone` param; the `wasd` test helper.
- Modify: `src/dispatcher.rs` — `handle_joystick` (global deadzone, no mode filter).
- Modify: `src/runtime.rs` — drop the mouse-mode warning.
- Modify: `src/monitor/mod.rs` — `render_monitor` joystick panel (deadzone from global; show real directions / "(unset)").
- Test: `src/config.rs`, `src/joystick.rs`.

**Interfaces:**
- Consumes: `ProfileSet::joystick_deadzone()` (Task 1).
- Produces: `JoystickConfig { up, down, left, right: Option<String> }`; `Joystick::update(&mut self, x: u8, y: u8, cfg: &JoystickConfig, deadzone: u8) -> Vec<HoldAction>`; `Profile::set_joystick(&mut self, Option<JoystickConfig>)`.

- [ ] **Step 1: Write the failing tests**

Add to `src/config.rs` `mod tests`:

```rust
    #[test]
    fn joystick_parses_directions_only() {
        let src = "[joystick]\nup = \"w\"\nleft = \"a\"\n";
        let raw: RawConfig = toml::from_str(src).unwrap();
        let p = Profile::from_raw(raw).unwrap();
        let j = p.joystick().unwrap();
        assert_eq!(j.up.as_deref(), Some("w"));
        assert_eq!(j.left.as_deref(), Some("a"));
        assert_eq!(j.down, None);
    }

    #[test]
    fn legacy_joystick_with_mode_and_deadzone_loads() {
        let src = "[joystick]\nmode = \"mouse\"\ndeadzone = 200\nup = \"w\"\n";
        let raw: RawConfig = toml::from_str(src).unwrap();
        let p = Profile::from_raw(raw).unwrap(); // no error despite mouse + 200
        assert_eq!(p.joystick().unwrap().up.as_deref(), Some("w"));
    }

    #[test]
    fn to_toml_joystick_directions_only() {
        let src = "[joystick]\nmode = \"wasd\"\ndeadzone = 30\nup = \"w\"\n";
        let p = Profile::from_raw(toml::from_str(src).unwrap()).unwrap();
        let toml = p.to_toml().unwrap();
        assert!(toml.contains("up = \"w\""));
        assert!(!toml.contains("mode"));
        assert!(!toml.contains("deadzone"));
    }

    #[test]
    fn to_toml_omits_empty_joystick() {
        // A joystick with all directions None serializes no [joystick] table.
        let mut p = Profile::from_raw(raw(&[("G1", "a")])).unwrap();
        p.set_joystick(Some(JoystickConfig { up: None, down: None, left: None, right: None }));
        assert!(!p.to_toml().unwrap().contains("[joystick]"));
    }
```

Add to `src/joystick.rs` tests: update every existing test that calls `update(x, y, &cfg)` to `update(x, y, &cfg, 30)` and every `wasd()` helper call to build the deadzone-less config (see Step 3). Add:

```rust
    #[test]
    fn deadzone_param_gates_direction() {
        let mut j = Joystick::new();
        let cfg = JoystickConfig { up: Some("w".into()), down: Some("s".into()),
                                   left: Some("a".into()), right: Some("d".into()) };
        // within deadzone (center ± 40 with dz=50) → no action
        let actions = j.update(127 - 40, 127, &cfg, 50);
        assert!(actions.is_empty());
    }
```

- [ ] **Step 2: Run tests to verify they fail / the crate won't compile**

Run: `cargo test 2>&1 | head -30`
Expected: compile errors on the `JoystickConfig`/`update` signature change (the ripple to fix in Step 3) + the new assertions.

- [ ] **Step 3: Implement**

In `src/config.rs`:

Change `RawJoystick` so `mode`/`deadzone`/directions are all optional + skip-serialized:

```rust
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RawJoystick {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadzone: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub up: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub down: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<String>,
}
```

Delete `default_mode`/`default_deadzone` and the `JoystickMode` enum. Simplify `JoystickConfig`:

```rust
#[derive(Debug, Clone)]
pub struct JoystickConfig {
    pub up: Option<String>,
    pub down: Option<String>,
    pub left: Option<String>,
    pub right: Option<String>,
}
```

Rewrite `parse_joystick` (ignore mode/deadzone):

```rust
fn parse_joystick(rj: RawJoystick) -> Result<JoystickConfig> {
    Ok(JoystickConfig { up: rj.up, down: rj.down, left: rj.left, right: rj.right })
}
```

Rewrite the `to_toml` joystick block (directions only; omit when all empty):

```rust
        let joystick = self.joystick.as_ref()
            .filter(|j| j.up.is_some() || j.down.is_some() || j.left.is_some() || j.right.is_some())
            .map(|j| RawJoystick {
                mode: None, deadzone: None,
                up: j.up.clone(), down: j.down.clone(), left: j.left.clone(), right: j.right.clone(),
            });
```

Add a setter to `impl Profile` (near `joystick()`):

```rust
    pub fn set_joystick(&mut self, joystick: Option<JoystickConfig>) {
        self.joystick = joystick;
    }
```

In `src/joystick.rs`: change `update` to take `deadzone: u8` and use it instead of `cfg.deadzone`:

```rust
    pub fn update(&mut self, x: u8, y: u8, cfg: &JoystickConfig, deadzone: u8) -> Vec<HoldAction> {
        // … replace cfg.deadzone with deadzone in the two target(...) calls …
        let want_x = Self::target(x, deadzone, &cfg.left, &cfg.right);
        let want_y = Self::target(y, deadzone, &cfg.up, &cfg.down);
        // … rest unchanged …
```

Update the `wasd` test helper in `joystick.rs` to the new config shape (drop `mode`/`deadzone` fields):

```rust
    fn wasd() -> JoystickConfig {
        JoystickConfig { up: Some("w".into()), down: Some("s".into()),
                         left: Some("a".into()), right: Some("d".into()) }
    }
```
…and its callers pass the deadzone as the 4th `update` arg (e.g. `update(x, y, &wasd(), 30)`).

In `src/dispatcher.rs` `handle_joystick` — snapshot directions + global deadzone, drop the mode filter:

```rust
    fn handle_joystick(&mut self, x: u8, y: u8) {
        let (cfg, deadzone) = {
            let set = self.profiles.read().unwrap();
            (set.active_profile().and_then(|p| p.joystick()).cloned(), set.joystick_deadzone())
        };
        let actions = match &cfg {
            Some(jc) => self.joystick.update(x, y, jc, deadzone),
            None => Vec::new(),
        };
        self.apply(actions);
    }
```

In `src/runtime.rs`: remove the block that warns about `JoystickMode::Mouse` (it referenced the now-deleted enum). Delete those lines (the `if let Some(j) = …active_profile().joystick() { if j.mode == Mouse … }` warning in `run_headless`).

In `src/monitor/mod.rs`, the **bottom status panel** (in the `update()` method's `TopBottomPanel::bottom(...)`) currently builds a joystick string using `j.mode` and `j.deadzone` — those fields are gone. Replace that with the global deadzone (or an on/off note):

```rust
            let set = self.profiles.read().unwrap();
            let joy = if set.active_profile().and_then(|c| c.joystick()).is_some() {
                format!("joystick: on (deadzone {})", set.joystick_deadzone())
            } else {
                "joystick: off".to_string()
            };
            ui.label(format!("config.toml · {joy}"));
```

(Grep for any other `.mode` / `.deadzone` / `JoystickMode` references and update them — these are the only two files that use them beyond `config.rs`/`joystick.rs`.)

In `src/monitor/mod.rs` `render_monitor` joystick panel: replace the `.unwrap_or((30, "w".into(), ...))` fallback. Read directions from the profile and deadzone from the global:

```rust
                        let joy = cfg.and_then(|c| c.joystick());
                        let dz = set.joystick_deadzone();
                        let up = joy.and_then(|j| j.up.clone());
                        let down = joy.and_then(|j| j.down.clone());
                        let left = joy.and_then(|j| j.left.clone());
                        let right = joy.and_then(|j| j.right.clone());
```
…and where the direction labels render, show the real value or a dimmed "(unset)":

```rust
                        let unset = egui::Color32::from_gray(90);
                        let show = |ui: &mut egui::Ui, arrow: &str, v: &Option<String>, active: bool| {
                            match v {
                                Some(s) => { ui.colored_label(if active { hot } else { dim }, format!("{arrow}{s}")); }
                                None => { ui.colored_label(unset, format!("{arrow}(unset)")); }
                            }
                        };
                        ui.horizontal(|ui| {
                            show(ui, "↑", &up, a_up);
                            show(ui, "↓", &down, a_down);
                            show(ui, "←", &left, a_left);
                            show(ui, "→", &right, a_right);
                        });
```
(Use `dz` for the deadzone circle radius: `let dz_r = rect.width() * (dz as f32 / 255.0);` and the `a_up`/`a_down`/`a_left`/`a_right` computations use `dz` instead of the old per-profile deadzone.)

**Existing tests + references to update** (mode/deadzone are gone — these will break and must be fixed as part of this atomic task):
- `src/dispatcher.rs`: remove `JoystickMode` from the `use crate::config::{…}` import; update the `config_with_joystick` test helper if it builds a `JoystickConfig` with `mode`/`deadzone` fields (drop them).
- `src/config.rs` `mod tests`: `parses_joystick_section` (drop the `j.mode`/`j.deadzone` asserts, keep direction asserts); **remove** the now-obsolete `joystick_mode_defaults_to_wasd`, `deadzone_default_is_30`, `deadzone_over_127_is_error`, and `unknown_joystick_mode_is_error` tests (parse no longer carries mode or validates deadzone).
- `src/config.rs` `mod profileset_tests`: `save_active_bindings_writes_and_preserves_others` — drop the `j.deadzone == 20` assert (keep `joystick().is_some()`).
- Any `JoystickConfig { mode, deadzone, … }` literal in tests → the new `{ up, down, left, right }` shape.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test` then `cargo build`
Expected: PASS — all green; `cargo build` clean except the pre-existing `usb.rs` warning. Fix any remaining `JoystickConfig { … }` / `.mode` / `.deadzone` / `JoystickMode` reference the compiler flags (grep to confirm none remain).

- [ ] **Step 5: Commit**

```bash
git add src/config.rs src/joystick.rs src/dispatcher.rs src/runtime.rs src/monitor/mod.rs
git commit -m "feat: joystick directions-only + global deadzone consumers"
```

---

### Task 3: `save_active_bindings` persists joystick directions

**Files:**
- Modify: `src/config.rs` — `save_active_bindings` signature + body; update its existing tests; the monitor Save caller (temporary `None`).
- Test: `src/config.rs` (`mod profileset_tests`).

**Interfaces:**
- Consumes: `Profile::set_joystick` (Task 2).
- Produces: `save_active_bindings(bindings, repeat, labels, joystick: Option<JoystickConfig>) -> Result<()>`.

- [ ] **Step 1: Write the failing test + update existing callers**

```rust
    #[test]
    fn save_active_bindings_writes_joystick() {
        let d = tmp("save-joy");
        write(&d.join("profiles"), "p.toml", "[keys]\nG1 = \"a\"\n");
        write(&d, "config.toml", "profiles_dir = \"profiles\"\nm1 = \"p.toml\"\n");
        let mut set = ProfileSet::load(&d.join("config.toml")).unwrap();
        let joy = Some(JoystickConfig { up: Some("w".into()), down: Some("s".into()),
                                        left: Some("a".into()), right: Some("d".into()) });
        set.save_active_bindings(HashMap::new(), HashMap::new(), HashMap::new(), joy).unwrap();
        let text = std::fs::read_to_string(d.join("profiles/p.toml")).unwrap();
        assert!(text.contains("[joystick]"));
        assert!(text.contains("up = \"w\""));
    }
```

Update the FOUR existing `save_active_bindings(...)` calls in `mod profileset_tests` (from Tasks in the labels feature: `save_active_bindings_writes_and_preserves_others`, `saving_github_profile_marks_modified`, `saving_user_profile_stays_clean`, `save_active_bindings_writes_labels`) to pass a 4th argument `None`.

- [ ] **Step 2: Run to verify fail** — `cargo test save_active_bindings_writes_joystick` → FAIL (arity).

- [ ] **Step 3: Implement**

Extend the signature and set the joystick before the modified-flip:

```rust
    pub fn save_active_bindings(
        &mut self,
        bindings: HashMap<G13Key, String>,
        repeat: HashMap<G13Key, bool>,
        labels: HashMap<G13Key, String>,
        joystick: Option<JoystickConfig>,
    ) -> Result<()> {
        if self.active_name().is_none() || self.active_profile().is_none() {
            anyhow::bail!("no profile in the active slot");
        }
        let path = self.active_path();
        let profile = self.active_profile_mut().expect("checked above");
        profile.set_bindings(bindings);
        profile.set_repeat(repeat);
        profile.set_labels(labels);
        profile.set_joystick(joystick);
        if profile.source() == ProfileSource::Github {
            profile.set_modified(true);
        }
        let toml = profile.to_toml()?;
        std::fs::write(&path, toml)
            .with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }
```

Update the single monitor caller (Save button in `render_bindings`) to pass `None` for now (Task 5 wires it):

```rust
                    match self.profiles.write().unwrap().save_active_bindings(bindings, repeat, labels, None) {
```

- [ ] **Step 4: Run to verify pass** — `cargo test` (all green) + `cargo build` clean.

- [ ] **Step 5: Commit**

```bash
git add src/config.rs src/monitor/mod.rs
git commit -m "feat: save_active_bindings persists joystick directions"
```

---

### Task 4: Settings tab — global deadzone slider

GUI — **manual-verify, no unit tests**.

**Files:**
- Modify: `src/monitor/mod.rs` — `render_settings`.

**Interfaces:**
- Consumes: `ProfileSet::{joystick_deadzone, set_joystick_deadzone, persist_joystick_deadzone}` (Task 1).

- [ ] **Step 1: Add the slider**

In `render_settings`, add a Joystick deadzone control (snapshot the value, show a slider, apply on change through short locks — never hold a lock across the persist):

```rust
        ui.add_space(8.0);
        ui.separator();
        ui.label("Joystick deadzone");
        let mut dz = self.profiles.read().unwrap().joystick_deadzone();
        if ui.add(egui::Slider::new(&mut dz, 0..=127)).changed() {
            self.profiles.write().unwrap().set_joystick_deadzone(dz);
            if let Err(e) = self.profiles.read().unwrap().persist_joystick_deadzone(dz) {
                log::warn!("persist deadzone failed: {e:#}");
            }
        }
        ui.weak("Distance the stick must move from center before a direction fires (applies to all profiles).");
```

(`render_settings` is `&self`; all mutation goes through the `self.profiles` `Arc`, so `&self` is fine. Do not hold the write guard across the `persist` call — they are separate statements/locks above.)

- [ ] **Step 2: Build + manual smoke**

Run: `cargo build` then `cargo test` (unchanged count).

Manual (record for the milestone): the Settings tab shows a deadzone slider at the current value; dragging it persists to `config.toml`'s `[joystick] deadzone` and the Monitor's deadzone circle updates.

- [ ] **Step 3: Commit**

```bash
git add src/monitor/mod.rs
git commit -m "feat: Settings tab — global joystick deadzone slider"
```

---

### Task 5: Bindings tab — joystick direction editor

GUI — **manual-verify, no unit tests**.

**Files:**
- Modify: `src/monitor/mod.rs` — `MonitorApp` field `joy_edits`; the buffer-reload block; a joystick section in `render_bindings`; the Save button.

**Interfaces:**
- Consumes: `Profile::joystick()`, `JoystickConfig`, `save_active_bindings(..., joystick)` (Task 3), `combo_valid`.

- [ ] **Step 1: Buffer + reload**

Add a field to `MonitorApp`: `joy_edits: [String; 4],` (order: up, down, left, right), initialized `[String::new(), String::new(), String::new(), String::new()]` in `MonitorApp::new`.

In the buffer-reload block (where `self.edits`/`self.repeat_edits`/`self.label_edits` are rebuilt), add:

```rust
                let j = profile.joystick();
                self.joy_edits = [
                    j.and_then(|j| j.up.clone()).unwrap_or_default(),
                    j.and_then(|j| j.down.clone()).unwrap_or_default(),
                    j.and_then(|j| j.left.clone()).unwrap_or_default(),
                    j.and_then(|j| j.right.clone()).unwrap_or_default(),
                ];
```

- [ ] **Step 2: Render the joystick section**

After the thumb-button rows in `render_bindings` (inside the same `ScrollArea`, after the THUMB loop), add a Joystick section:

```rust
            ui.add_space(6.0);
            ui.separator();
            ui.label("Joystick");
            let dim = egui::Color32::from_gray(110);
            let green = egui::Color32::from_rgb(127, 224, 160);
            let red = egui::Color32::from_rgb(220, 90, 90);
            for (i, name) in ["Up", "Down", "Left", "Right"].iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.monospace(format!("{name:>5}"));
                    ui.add_space(6.0);
                    ui.add(egui::TextEdit::singleline(&mut self.joy_edits[i]).desired_width(160.0));
                    let b = &self.joy_edits[i];
                    let (mark, color) = if b.is_empty() { ("—", dim) }
                        else if combo_valid(b, &valid_keys) { ("ok", green) } else { ("bad", red) };
                    ui.colored_label(color, mark);
                });
            }
```

(`valid_keys` is already built once per frame in `render_bindings`. If the borrow checker objects to `&mut self.joy_edits[i]` inside the closure while `valid_keys` is borrowed, read `valid_keys` membership into a local `bool` before the `ui.horizontal` closure, mirroring how `render_binding_row` handles it.)

- [ ] **Step 3: Pass joystick to Save**

In the Save handler, build the joystick from `joy_edits` (empty strings → None directions; all-None ⇒ `None` so no `[joystick]`), and pass it:

```rust
                let mk = |s: &str| -> Option<String> { let s = s.trim(); if s.is_empty() { None } else { Some(s.to_string()) } };
                let jc = JoystickConfig {
                    up: mk(&self.joy_edits[0]), down: mk(&self.joy_edits[1]),
                    left: mk(&self.joy_edits[2]), right: mk(&self.joy_edits[3]),
                };
                let joystick = if jc.up.is_some() || jc.down.is_some() || jc.left.is_some() || jc.right.is_some() {
                    Some(jc)
                } else { None };
                match self.profiles.write().unwrap().save_active_bindings(bindings, repeat, labels, joystick) {
```

(Import `JoystickConfig` at the top of `src/monitor/mod.rs` if not already: `use crate::config::JoystickConfig;`.)

- [ ] **Step 4: Build + manual smoke**

Run: `cargo build` then `cargo test` (unchanged count).

Manual: the Bindings tab shows a Joystick section with Up/Down/Left/Right combo fields, pre-filled from the active profile; editing + Save writes the `[joystick]` directions (and clearing all four removes the `[joystick]` section); an invalid combo shows "bad".

- [ ] **Step 5: Commit**

```bash
git add src/monitor/mod.rs
git commit -m "feat: Bindings tab — joystick direction editor"
```

---

### Task 6: Monitor layout — M-keys on top, thumb cells left of the joystick

GUI — **manual-verify, no unit tests**.

**Files:**
- Modify: `src/monitor/mod.rs` — `render_monitor`.

- [ ] **Step 1: Move the M-keys row to the top**

In `render_monitor`, the M-keys indicator row (`ui.horizontal(|ui| { ui.label("M-keys:"); … })`) currently renders at the BOTTOM. Move that block to render BEFORE the `for row in ROWS` grid loop (just after `ui.set_width(BLOCK_W);`), so it sits above the key grid. Add a small `ui.add_space(6.0)` after it.

- [ ] **Step 2: Render thumb buttons as cells left of the joystick**

Remove the old bottom `ui.horizontal(|ui| { ui.label("Thumb:"); … })` text row. In the bottom area (where the joystick panel renders), wrap the thumb cells and the joystick in a single `ui.horizontal` so thumb sits to the LEFT of the joystick. Reuse the same cell rendering as the G-keys (key · combo · label) for `Btn1`, `Btn2`, `Stick`, stacked vertically:

```rust
                ui.horizontal(|ui| {
                    ui.add_space((BLOCK_W - 140.0 - 70.0) * 0.5); // rough centering for thumb col + joystick
                    // Thumb column (left)
                    ui.vertical(|ui| {
                        for &key in &[G13Key::Btn1, G13Key::Btn2, G13Key::Stick] {
                            let pressed = snapshot.pressed.contains(&key);
                            let binding = cfg.and_then(|c| c.get_binding(key)).unwrap_or("—");
                            let label = cfg.and_then(|c| c.label(key)).unwrap_or("");
                            let fill = if pressed { egui::Color32::from_rgb(20, 54, 31) } else { egui::Color32::from_gray(38) };
                            egui::Frame::new().fill(fill).inner_margin(4.0).outer_margin(3.0).corner_radius(4.0).show(ui, |ui| {
                                ui.set_width(48.0);
                                ui.vertical(|ui| {
                                    ui.strong(format!("{key:?}"));
                                    ui.add(egui::Label::new(egui::RichText::new(binding).small()).truncate());
                                    ui.add(egui::Label::new(egui::RichText::new(label).small().weak()).truncate());
                                });
                            });
                        }
                    });
                    ui.add_space(10.0);
                    // Joystick panel (right) — the existing viz + direction labels from Task 2
                    ui.vertical(|ui| {
                        ui.label("JOYSTICK");
                        // … the joystick viz + show(...) direction labels (moved here from its current spot) …
                    });
                });
```

Move the joystick panel's inner code (the `allocate_painter` viz + the `show(...)` direction labels from Task 2) into the right-hand `ui.vertical` above. The M-keys row is now at the top (Step 1); the bottom is `[thumb column] [joystick]`.

- [ ] **Step 3: Build + manual smoke**

Run: `cargo build` then `cargo test` (unchanged count).

Manual: the Monitor shows the M-keys row at the top (active highlighted); the thumb buttons (BTN1/BTN2/STICK) render as cells with combo+label to the left of the joystick; the joystick shows the profile's real directions (or "(unset)"); pressing a thumb button highlights its cell.

- [ ] **Step 4: Commit**

```bash
git add src/monitor/mod.rs
git commit -m "feat: Monitor — M-keys on top, thumb cells beside the joystick"
```

---

### Task 7: Shipped content — global deadzone + clean profile joysticks

**Files:**
- Modify: `config.toml`, `profiles/basic.toml`, `catalog/gaming.toml`.

- [ ] **Step 1: `config.toml`** — add the global deadzone. Append (or place near the top, after the `mN` lines):

```toml
[joystick]
deadzone = 30
```

- [ ] **Step 2: `profiles/basic.toml`** — in its `[joystick]` table, remove the `mode` and `deadzone` lines, keeping only `up`/`down`/`left`/`right`. Result:

```toml
[joystick]
up = "w"
down = "s"
left = "a"
right = "d"
```

- [ ] **Step 3: `catalog/gaming.toml`** — same: strip `mode`/`deadzone` from its `[joystick]`, keep the four directions.

- [ ] **Step 4: Verify load**

Run: `cargo test` (must stay green — config tests use temp files). Confirm both profiles + the manifest still parse: if unsure, add a throwaway
`Profile::load(Path::new("profiles/basic.toml")).unwrap()` + `ProfileSet::load(Path::new("config.toml"))...` test, run, then remove it.

- [ ] **Step 5: Commit**

```bash
git add config.toml profiles/basic.toml catalog/gaming.toml
git commit -m "feat: global deadzone in config.toml; clean profile joystick sections"
```

---

## Notes for the executor

- `cargo test` (never `--lib`). Tasks 1–3 are unit-tested; Task 2 is the atomic type-change ripple (touches 5 files, one commit); Tasks 4–6 are GUI manual-verify; Task 7 is content.
- After all tasks: final whole-branch review, then `superpowers:finishing-a-development-branch`. The manual smoke items (Tasks 4–6) become the milestone smoke-test checklist. Before a GUI smoke test, sync the beside-exe bundle (`target/release/config.toml` + `profiles/`) to the repo.
