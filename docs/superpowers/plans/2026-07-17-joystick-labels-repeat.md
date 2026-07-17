# Joystick Labels + Repeat — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give each joystick direction an optional label and an auto-repeat flag (like G-keys), stored per-profile, applied by the dispatcher's auto-repeat loop, and editable in the Bindings tab.

**Architecture:** Labels/repeat live on `Profile` (parallel to G-key `[labels]`/`[repeat]`), keyed by a new `JoystickDir` enum, serialized as nested `[joystick.labels]`/`[joystick.repeat]` tables. `JoystickMapper` starts reporting which direction fired so the dispatcher can register repeat-enabled held directions into a parallel `joystick_held` repeat map that `tick()` re-fires. The GUI Bindings joystick editor gains a label field + repeat checkbox per direction.

**Tech Stack:** Rust, `toml`/serde, `egui`. No new dependencies.

## Global Constraints

- **GNU toolchain only.** Build/test with `stable-x86_64-pc-windows-gnu`; MinGW gcc at `C:\Strawberry\c\bin\gcc.exe`. If `cargo`/`gcc` not found, prepend to PATH per CLAUDE.md. Do NOT switch to the MSVC target.
- **TDD** for pure/config/dispatch logic. GUI code is the no-unit-test exception (manual verify).
- **Backward-compatible:** existing profiles with a plain `[joystick]` (no `labels`/`repeat` sub-tables) must load unchanged (empty maps, repeat off, no behavior change).
- **Error policy:** no `panic!`/`unwrap()` on the runtime path beyond the accepted lock idiom. An unknown direction key in `[joystick.labels]`/`[joystick.repeat]` is a **load error** (mirrors `[labels]`/`[repeat]` unknown-key errors).
- **Repeat timing** = the global `[autorepeat]` delay/interval (same as G-keys); no per-direction timing.
- Direction keys are lowercase `up`/`down`/`left`/`right`.
- One focused commit per task; imperative subject line.

---

## File Structure
- **Modify** `src/config.rs` — `JoystickDir` enum; `RawJoystick` labels/repeat; `Profile` joystick label/repeat storage + accessors + setters; `from_raw` parse; `to_toml` emit; `save_active_bindings` params.
- **Modify** `src/joystick.rs` — `HoldAction` carries `JoystickDir`; `update`/`release_all` report it.
- **Modify** `src/dispatcher.rs` — `joystick_held` repeat map; `handle_joystick` registers/deregisters; `tick` re-fires; releases clear.
- **Modify** `src/monitor/mod.rs` — Bindings-tab joystick editor: label field + repeat checkbox per direction; load + save.
- **Modify** `milestones/open/joystick-labels-repeat.md` → `ongoing/` with smoke checklist.

---

## Task 1: `JoystickDir` + read path (parse + accessors)

**Files:**
- Modify: `src/config.rs`
- Test: `src/config.rs` (`profileset_tests`)

**Interfaces:**
- Produces: `pub enum JoystickDir { Up, Down, Left, Right }` (derives `Debug, Clone, Copy, PartialEq, Eq, Hash`) with `pub fn parse_joystick_dir(&str) -> Option<JoystickDir>` (lowercase `up`/`down`/`left`/`right`) and `JoystickDir::as_str(&self) -> &'static str`.
- On `Profile`: fields `joystick_labels: HashMap<JoystickDir, String>`, `joystick_repeat: HashMap<JoystickDir, bool>`; accessors `pub fn joystick_label(&self, dir: JoystickDir) -> Option<&str>`, `pub fn joystick_repeats(&self, dir: JoystickDir) -> bool`.

- [ ] **Step 1: Write the failing test**

Add to `profileset_tests` in `src/config.rs`:

```rust
#[test]
fn joystick_labels_and_repeat_parse() {
    let raw: RawConfig = toml::from_str(
        "[joystick]\nup = \"w\"\ndown = \"s\"\n\
         [joystick.labels]\nup = \"Forward\"\n\
         [joystick.repeat]\ndown = true\n").unwrap();
    let p = Profile::from_raw(raw).unwrap();
    assert_eq!(p.joystick_label(JoystickDir::Up), Some("Forward"));
    assert_eq!(p.joystick_label(JoystickDir::Down), None);
    assert!(p.joystick_repeats(JoystickDir::Down));
    assert!(!p.joystick_repeats(JoystickDir::Up));
}

#[test]
fn joystick_without_labels_repeat_is_backward_compatible() {
    let raw: RawConfig = toml::from_str("[joystick]\nup = \"w\"\n").unwrap();
    let p = Profile::from_raw(raw).unwrap();
    assert_eq!(p.joystick_label(JoystickDir::Up), None);
    assert!(!p.joystick_repeats(JoystickDir::Up));
}

#[test]
fn joystick_unknown_direction_key_is_error() {
    let raw: RawConfig = toml::from_str(
        "[joystick]\nup = \"w\"\n[joystick.labels]\ndiagonal = \"x\"\n").unwrap();
    assert!(Profile::from_raw(raw).is_err());
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test config::profileset_tests::joystick_labels_and_repeat_parse`
Expected: FAIL — `cannot find type JoystickDir` / `no method joystick_label`.

- [ ] **Step 3: Implement**

In `src/config.rs`:

Add the enum (near `JoystickConfig`):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JoystickDir { Up, Down, Left, Right }

impl JoystickDir {
    pub fn as_str(&self) -> &'static str {
        match self { Self::Up => "up", Self::Down => "down", Self::Left => "left", Self::Right => "right" }
    }
}

pub fn parse_joystick_dir(s: &str) -> Option<JoystickDir> {
    match s.to_ascii_lowercase().as_str() {
        "up" => Some(JoystickDir::Up),
        "down" => Some(JoystickDir::Down),
        "left" => Some(JoystickDir::Left),
        "right" => Some(JoystickDir::Right),
        _ => None,
    }
}
```

Extend `RawJoystick` (add fields after `right`):

```rust
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<std::collections::HashMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat: Option<std::collections::HashMap<String, bool>>,
```

Add fields to `struct Profile` (after `labels`):

```rust
    joystick_labels: HashMap<JoystickDir, String>,
    joystick_repeat: HashMap<JoystickDir, bool>,
```

In `Profile::from_raw`, after computing `joystick`, parse the maps from the raw joystick (which was consumed by `parse_joystick` — so read them BEFORE moving `rj`). Replace the `let joystick = match raw.joystick { ... }` block with:

```rust
        let mut joystick_labels = HashMap::new();
        let mut joystick_repeat = HashMap::new();
        let joystick = match raw.joystick {
            Some(rj) => {
                if let Some(labels) = &rj.labels {
                    for (name, text) in labels {
                        let dir = parse_joystick_dir(name)
                            .with_context(|| format!("unknown joystick direction in [joystick.labels]: {name}"))?;
                        let text = text.trim().to_string();
                        if !text.is_empty() { joystick_labels.insert(dir, text); }
                    }
                }
                if let Some(rep) = &rj.repeat {
                    for (name, on) in rep {
                        let dir = parse_joystick_dir(name)
                            .with_context(|| format!("unknown joystick direction in [joystick.repeat]: {name}"))?;
                        joystick_repeat.insert(dir, *on);
                    }
                }
                Some(parse_joystick(rj)?)
            }
            None => None,
        };
```

Add `joystick_labels, joystick_repeat` to the `Ok(Self { ... })` in `from_raw` AND to `impl Default for Profile`.

Add accessors in `impl Profile`:

```rust
    pub fn joystick_label(&self, dir: JoystickDir) -> Option<&str> {
        self.joystick_labels.get(&dir).map(String::as_str)
    }
    pub fn joystick_repeats(&self, dir: JoystickDir) -> bool {
        self.joystick_repeat.get(&dir).copied().unwrap_or(false)
    }
```

- [ ] **Step 4: Run tests + build**

Run: `cargo test config::profileset_tests` then `cargo build`
Expected: the 3 new tests pass; build clean (existing `Profile { ... }` literals now need the two new fields — fix any that fail to compile by adding `joystick_labels: HashMap::new(), joystick_repeat: HashMap::new()`; the only non-Default literals are in `from_raw` and `Default`).

- [ ] **Step 5: Commit**

```bash
git add src/config.rs
git commit -m "feat(config): parse per-direction joystick labels + repeat"
```

---

## Task 2: Write path (to_toml + setters + save_active_bindings)

**Files:**
- Modify: `src/config.rs`
- Test: `src/config.rs` (`profileset_tests`)

**Interfaces:**
- On `Profile`: `pub fn set_joystick_labels(&mut self, m: HashMap<JoystickDir, String>)`, `pub fn set_joystick_repeat(&mut self, m: HashMap<JoystickDir, bool>)`.
- `save_active_bindings` gains two params: `joystick_labels: HashMap<JoystickDir, String>`, `joystick_repeat: HashMap<JoystickDir, bool>`.

- [ ] **Step 1: Write the failing test**

Add to `profileset_tests`:

```rust
#[test]
fn joystick_labels_repeat_round_trip_to_toml() {
    let raw: RawConfig = toml::from_str("[joystick]\nup = \"w\"\ndown = \"s\"\n").unwrap();
    let mut p = Profile::from_raw(raw).unwrap();
    let mut labels = HashMap::new();
    labels.insert(JoystickDir::Up, "Forward".to_string());
    let mut repeat = HashMap::new();
    repeat.insert(JoystickDir::Down, true);
    p.set_joystick_labels(labels);
    p.set_joystick_repeat(repeat);

    let toml = p.to_toml().unwrap();
    let reloaded = Profile::from_raw(toml::from_str(&toml).unwrap()).unwrap();
    assert_eq!(reloaded.joystick_label(JoystickDir::Up), Some("Forward"));
    assert!(reloaded.joystick_repeats(JoystickDir::Down));
    assert!(toml.contains("[joystick.labels]"));
    assert!(toml.contains("[joystick.repeat]"));
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test config::profileset_tests::joystick_labels_repeat_round_trip_to_toml`
Expected: FAIL — `no method set_joystick_labels`.

- [ ] **Step 3: Implement**

Add setters in `impl Profile`:

```rust
    pub fn set_joystick_labels(&mut self, m: HashMap<JoystickDir, String>) {
        self.joystick_labels = m;
    }
    pub fn set_joystick_repeat(&mut self, m: HashMap<JoystickDir, bool>) {
        self.joystick_repeat = m;
    }
```

In `to_toml`, where the `joystick` `RawJoystick` is built, populate its `labels`/`repeat` (filter empty). Change the `.map(|j| RawJoystick { ... })` closure to also set:

```rust
                labels: {
                    let m: std::collections::HashMap<String, String> = self.joystick_labels.iter()
                        .filter(|(_, v)| !v.trim().is_empty())
                        .map(|(d, v)| (d.as_str().to_string(), v.clone()))
                        .collect();
                    if m.is_empty() { None } else { Some(m) }
                },
                repeat: {
                    let m: std::collections::HashMap<String, bool> = self.joystick_repeat.iter()
                        .filter(|(_, &v)| v)
                        .map(|(d, _)| (d.as_str().to_string(), true))
                        .collect();
                    if m.is_empty() { None } else { Some(m) }
                },
```

**Important:** joystick labels/repeat only serialize when the `[joystick]` table exists (the `.filter(|j| j.up.is_some() || ...)` guard). That's acceptable — labels/repeat without any direction key are meaningless. If ALL direction keys are empty but labels exist, they are dropped; this matches the existing "no joystick keys → no `[joystick]`" behavior.

Extend `save_active_bindings` signature and body:

```rust
    pub fn save_active_bindings(
        &mut self,
        bindings: HashMap<G13Key, String>,
        repeat: HashMap<G13Key, bool>,
        labels: HashMap<G13Key, String>,
        joystick: Option<JoystickConfig>,
        joystick_labels: HashMap<JoystickDir, String>,
        joystick_repeat: HashMap<JoystickDir, bool>,
    ) -> Result<()> {
```

After `profile.set_joystick(joystick);` add:

```rust
        profile.set_joystick_labels(joystick_labels);
        profile.set_joystick_repeat(joystick_repeat);
```

(The one non-test call site — the GUI Bindings Save — is updated in Task 5. Existing `save_active_bindings` test call sites in `profileset_tests` must pass two extra `HashMap::new()` args to compile.)

- [ ] **Step 4: Run tests + build**

Run: `cargo test config::profileset_tests` then `cargo build`
Expected: new test passes; existing `save_active_bindings(...)` test calls updated with two `HashMap::new()` args; build clean except the GUI call site (fixed in Task 5) — if the GUI breaks the build now, add the two `HashMap::new()` args at `src/monitor/mod.rs` save call as a stopgap, to be fleshed out in Task 5.

- [ ] **Step 5: Commit**

```bash
git add src/config.rs src/monitor/mod.rs
git commit -m "feat(config): serialize + save joystick labels/repeat"
```

---

## Task 3: `JoystickMapper` reports direction

**Files:**
- Modify: `src/joystick.rs`
- Test: `src/joystick.rs`

**Interfaces:**
- Consumes: `crate::config::JoystickDir`.
- Produces: `HoldAction` becomes `KeyDown { dir: JoystickDir, key: String }` / `KeyUp { dir: JoystickDir, key: String }`. `update`/`release_all` return these.

- [ ] **Step 1: Update the tests (RED)**

In `src/joystick.rs` tests, change assertions to the new shape, e.g. `full_left_presses_a`:

```rust
    #[test]
    fn full_left_presses_a() {
        let mut m = JoystickMapper::new();
        assert_eq!(m.update(0, 127, &wasd(), 30),
            vec![HoldAction::KeyDown { dir: JoystickDir::Left, key: "a".into() }]);
    }
```

Update the other assertions similarly (`full_right`→Right/d, `full_up`→Up/w, `full_down`→Down/s, `return_to_center_releases`→`KeyUp { dir: Left, key: "a" }`, `diagonal_holds_two_keys`→contains `KeyDown{Left,"a"}` and `KeyDown{Up,"w"}`, `cross_center_left_to_right_swaps`→`[KeyUp{Left,"a"}, KeyDown{Right,"d"}]`, `release_all_lifts_held_keys`→`KeyUp{Left,"a"}`/`KeyUp{Up,"w"}`). Add `use crate::config::JoystickDir;` to the tests module.

Run: `cargo test joystick::` → FAIL (variant shape mismatch).

- [ ] **Step 2: Implement**

Rewrite `src/joystick.rs`'s type + mapper to carry direction. The axis state stores `(JoystickDir, String)`:

```rust
use crate::config::{JoystickConfig, JoystickDir};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HoldAction {
    KeyDown { dir: JoystickDir, key: String },
    KeyUp { dir: JoystickDir, key: String },
}

pub struct JoystickMapper {
    x_held: Option<(JoystickDir, String)>,
    y_held: Option<(JoystickDir, String)>,
}

const CENTER: i32 = 127;

impl JoystickMapper {
    pub fn new() -> Self { Self { x_held: None, y_held: None } }

    pub fn update(&mut self, x: u8, y: u8, cfg: &JoystickConfig, deadzone: u8) -> Vec<HoldAction> {
        let mut actions = Vec::new();
        let want_x = Self::target(x, deadzone, (JoystickDir::Left, &cfg.left), (JoystickDir::Right, &cfg.right));
        Self::diff(&mut actions, &mut self.x_held, want_x);
        let want_y = Self::target(y, deadzone, (JoystickDir::Up, &cfg.up), (JoystickDir::Down, &cfg.down));
        Self::diff(&mut actions, &mut self.y_held, want_y);
        actions
    }

    pub fn release_all(&mut self) -> Vec<HoldAction> {
        let mut actions = Vec::new();
        if let Some((dir, key)) = self.x_held.take() { actions.push(HoldAction::KeyUp { dir, key }); }
        if let Some((dir, key)) = self.y_held.take() { actions.push(HoldAction::KeyUp { dir, key }); }
        actions
    }

    fn target(value: u8, deadzone: u8, low: (JoystickDir, &Option<String>), high: (JoystickDir, &Option<String>))
        -> Option<(JoystickDir, String)> {
        let v = value as i32;
        let dz = deadzone as i32;
        if v < CENTER - dz {
            low.1.clone().map(|k| (low.0, k))
        } else if v > CENTER + dz {
            high.1.clone().map(|k| (high.0, k))
        } else {
            None
        }
    }

    fn diff(actions: &mut Vec<HoldAction>, held: &mut Option<(JoystickDir, String)>, want: Option<(JoystickDir, String)>) {
        if *held == want { return; }
        if let Some((dir, key)) = held.take() {
            actions.push(HoldAction::KeyUp { dir, key });
        }
        if let Some((dir, key)) = &want {
            actions.push(HoldAction::KeyDown { dir: *dir, key: key.clone() });
        }
        *held = want;
    }
}
```

- [ ] **Step 3: Run tests (GREEN)**

Run: `cargo test joystick::`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src/joystick.rs
git commit -m "feat(joystick): mapper reports which direction fired"
```

---

## Task 4: Dispatcher — joystick auto-repeat

**Files:**
- Modify: `src/dispatcher.rs`
- Test: `src/dispatcher.rs`

**Interfaces:**
- Consumes: `HoldAction::{KeyDown,KeyUp}` (dir-annotated), `Profile::joystick_repeats`, `ProfileSet::autorepeat`, `JoystickDir`.

- [ ] **Step 1: Write the failing test**

Add to `src/dispatcher.rs` tests. This uses the module's existing helpers (`tmp`, `write`, `MockInjector::new_with_holds()` — which records `key_down` calls into a `Vec<String>`). Read those first to confirm the exact APIs; they are stable in this module.

```rust
    #[test]
    fn joystick_repeat_refires_on_tick() {
        let d = tmp("joyrep");
        std::fs::create_dir_all(&d).unwrap();
        write(&d.join("config.toml"),
            "[keys]\n[joystick]\nup = \"w\"\ndown = \"s\"\n\
             [joystick.repeat]\nup = true\n\
             [autorepeat]\ndelay_ms = 0\ninterval_ms = 1\n");
        let config = Arc::new(RwLock::new(ProfileSet::load(&d.join("config.toml")).unwrap()));
        let (injector, holds) = MockInjector::new_with_holds();
        let mut disp = Dispatcher::new(config, Box::new(injector));

        // Full up -> key_down("w") once; repeat registered for Up.
        disp.handle(G13Event::JoystickMove { x: 127, y: 0 }).unwrap();
        assert_eq!(holds.lock().unwrap().clone(), vec!["w"]);

        // tick past the (zero) delay -> "w" re-fires.
        let t0 = Instant::now();
        disp.tick(t0);                              // schedules next_repeat
        disp.tick(t0 + Duration::from_millis(5));   // fires repeats
        assert!(holds.lock().unwrap().iter().filter(|k| *k == "w").count() >= 2,
            "up should auto-repeat: {:?}", holds.lock().unwrap());

        // Move to full down: Up releases (repeat entry cleared), Down has no repeat.
        holds.lock().unwrap().clear();
        disp.handle(G13Event::JoystickMove { x: 127, y: 255 }).unwrap();
        assert_eq!(holds.lock().unwrap().clone(), vec!["s"]); // down pressed once
        disp.tick(Instant::now() + Duration::from_millis(50));
        assert_eq!(holds.lock().unwrap().iter().filter(|k| *k == "s").count(), 1,
            "down must not repeat; up must have stopped");
    }
```

Assert: repeat-up re-fires on tick; after moving off Up, neither Up nor the non-repeat Down re-fires.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test dispatcher::` (new test) → FAIL.

- [ ] **Step 3: Implement**

Add a joystick repeat entry type + field:

```rust
struct JoyHeld {
    key: String,
    delay_ms: u64,
    interval_ms: u64,
    next_repeat: Option<Instant>,
}
```

Add `joystick_held: HashMap<crate::config::JoystickDir, JoyHeld>` to `Dispatcher` (init empty in `new`).

Rewrite `handle_joystick` to register/deregister repeats (snapshot repeat flags + autorepeat under the read lock):

```rust
    fn handle_joystick(&mut self, x: u8, y: u8) {
        let (cfg, deadzone, ar) = {
            let set = self.profiles.read().unwrap();
            (set.active_profile().and_then(|p| p.joystick()).cloned(),
             set.joystick_deadzone(), set.autorepeat())
        };
        let actions = match &cfg {
            Some(jc) => self.joystick.update(x, y, jc, deadzone),
            None => Vec::new(),
        };
        for action in actions {
            match action {
                HoldAction::KeyDown { dir, key } => {
                    if let Err(e) = self.injector.key_down(&key) {
                        log::warn!("joystick injection failed: {e:#}");
                    }
                    let repeats = {
                        let set = self.profiles.read().unwrap();
                        set.active_profile().map(|p| p.joystick_repeats(dir)).unwrap_or(false)
                    };
                    if repeats {
                        self.joystick_held.insert(dir, JoyHeld {
                            key, delay_ms: ar.delay_ms, interval_ms: ar.interval_ms, next_repeat: None,
                        });
                    }
                }
                HoldAction::KeyUp { dir, key } => {
                    if let Err(e) = self.injector.key_up(&key) {
                        log::warn!("joystick injection failed: {e:#}");
                    }
                    self.joystick_held.remove(&dir);
                }
            }
        }
    }
```

Update `apply` (used only by `release_joystick`/`release_all` now — it still receives `HoldAction`): change its match arms to the struct variants (`HoldAction::KeyDown { key, .. } => self.injector.key_down(key)`, `KeyUp { key, .. } => self.injector.key_up(key)`).

In `release_joystick`, after `self.apply(actions);` add `self.joystick_held.clear();`.

Extend `tick` to also re-fire joystick holds — after the existing `held_keys` repeat block, add the same schedule/fire logic over `self.joystick_held`:

```rust
        for held in self.joystick_held.values_mut() {
            match held.next_repeat {
                None => held.next_repeat = Some(now + Duration::from_millis(held.delay_ms)),
                Some(mut due) => {
                    while now >= due {
                        to_fire.push(held.key.clone());
                        due += Duration::from_millis(held.interval_ms);
                    }
                    held.next_repeat = Some(due);
                }
            }
        }
```

(`to_fire` is the same `Vec<String>` the G-key block builds; the shared `for key in to_fire { injector.key_down }` loop already fires them.)

- [ ] **Step 4: Run tests (GREEN) + full build**

Run: `cargo test dispatcher::` then `cargo test`
Expected: PASS; build clean.

- [ ] **Step 5: Commit**

```bash
git add src/dispatcher.rs
git commit -m "feat(dispatcher): auto-repeat repeat-flagged joystick directions"
```

---

## Task 5: GUI — Bindings joystick editor label + repeat

**Files:**
- Modify: `src/monitor/mod.rs`

**Interfaces:**
- Consumes: `Profile::{joystick_label, joystick_repeats}`; extended `save_active_bindings`; `JoystickDir`.

**Note:** GUI — manual verify, no unit test.

- [ ] **Step 1: Add edit state**

Add fields to `MonitorApp` (near `joy_edits`):

```rust
    joy_label_edits: [String; 4],
    joy_repeat_edits: [bool; 4],
```

Init in `MonitorApp::new` (near the `joy_edits` init):

```rust
            joy_label_edits: [String::new(), String::new(), String::new(), String::new()],
            joy_repeat_edits: [false; 4],
```

- [ ] **Step 2: Load them on profile (re)load**

Where `self.joy_edits = [...]` is populated in `render_bindings` (~line 1051), also populate the label/repeat edits from the active profile. The 4 indices map to Up, Down, Left, Right. Add, using the active profile (read under a short lock like the surrounding code):

```rust
                let dirs = [crate::config::JoystickDir::Up, crate::config::JoystickDir::Down,
                            crate::config::JoystickDir::Left, crate::config::JoystickDir::Right];
                let (jl, jr) = {
                    let set = self.profiles.read().unwrap();
                    match set.active_profile() {
                        Some(p) => (
                            dirs.map(|d| p.joystick_label(d).unwrap_or("").to_string()),
                            dirs.map(|d| p.joystick_repeats(d)),
                        ),
                        None => (Default::default(), [false; 4]),
                    }
                };
                self.joy_label_edits = jl;
                self.joy_repeat_edits = jr;
```

(Place this next to the existing `self.joy_edits = [...]` assignment; match the existing lock/borrow pattern there.)

- [ ] **Step 3: Render label + repeat per direction row**

In the joystick editor loop (~line 1176), extend each row after the key field + mark:

```rust
                    ui.add(egui::TextEdit::singleline(&mut self.joy_label_edits[i])
                        .desired_width(120.0).hint_text("label"));
                    ui.checkbox(&mut self.joy_repeat_edits[i], "repeat");
```

- [ ] **Step 4: Pass them on Save**

In the Save handler (~line 1110), build the joystick label/repeat maps (only for directions with a non-empty key) and pass to `save_active_bindings`:

```rust
                    let dirs = [crate::config::JoystickDir::Up, crate::config::JoystickDir::Down,
                                crate::config::JoystickDir::Left, crate::config::JoystickDir::Right];
                    let mut joystick_labels = HashMap::new();
                    let mut joystick_repeat = HashMap::new();
                    for (i, d) in dirs.iter().enumerate() {
                        if self.joy_edits[i].trim().is_empty() { continue; } // no key -> skip
                        let lbl = self.joy_label_edits[i].trim();
                        if !lbl.is_empty() { joystick_labels.insert(*d, lbl.to_string()); }
                        if self.joy_repeat_edits[i] { joystick_repeat.insert(*d, true); }
                    }
```

Change the save call to:

```rust
                    match self.profiles.write().unwrap().save_active_bindings(
                        bindings, repeat, labels, joystick, joystick_labels, joystick_repeat) {
```

- [ ] **Step 5: Build + manual check**

Run: `cargo build` then `cargo test`
Expected: clean build; tests pass.
Manual: `cargo run` → Bindings tab → each joystick direction row shows a key field, a `label` field, and a `repeat` checkbox; Save writes `[joystick.labels]`/`[joystick.repeat]` into the profile; reload shows them.

- [ ] **Step 6: Commit**

```bash
git add src/monitor/mod.rs
git commit -m "feat(gui): joystick label + repeat fields in the Bindings editor"
```

---

## Task 6: Milestone + hardware smoke test

**Files:**
- Move: `milestones/open/joystick-labels-repeat.md` → `milestones/ongoing/`

- [ ] **Step 1: Update + move the milestone**

Edit `milestones/open/joystick-labels-repeat.md`: set `Status: ongoing`, `Updated: 2026-07-17`, check the implemented boxes, and add:

```markdown
## Hardware smoke test (manual)
- [ ] A joystick direction with `repeat = true` auto-repeats its key while held
      (e.g. holding the stick fires the key repeatedly); `repeat = false` holds without repeating.
- [ ] Labels + repeat flags set in the Bindings tab save and survive a reload.
- [ ] An existing profile with a plain `[joystick]` still loads and works unchanged.
```

Then `git mv milestones/open/joystick-labels-repeat.md milestones/ongoing/joystick-labels-repeat.md`.

- [ ] **Step 2: Full build + test**

Run: `cargo test && cargo build --release`
Expected: all tests pass; release binary builds clean.

- [ ] **Step 3: Commit**

```bash
git add milestones/
git commit -m "docs: joystick-labels-repeat milestone to ongoing with smoke checklist"
```

---

## Self-Review

**Spec coverage:**
- Schema per-direction label + repeat, nested tables, backward-compat, unknown-dir error → Tasks 1, 2. ✓
- `JoystickMapper` reports direction → Task 3. ✓
- Dispatcher repeats held directions via `tick()` (global interval), clears on release → Task 4. ✓
- GUI label + repeat fields, save/reload → Task 5. ✓
- Smoke test → Task 6. ✓
- Out of scope (LCD labels = sub-project B; per-direction timing) → not implemented. ✓

**Placeholder scan:** Task 4's test is described (mirror the module's mock-injector harness) rather than fully written, because it must match the existing dispatcher test setup the implementer will read — the behavior to assert is fully specified. All other steps have complete code.

**Type consistency:** `JoystickDir` (Up/Down/Left/Right, Copy/Hash), `parse_joystick_dir`, `as_str`, `Profile::{joystick_label, joystick_repeats, set_joystick_labels, set_joystick_repeat}`, `HoldAction::{KeyDown,KeyUp}{dir,key}`, `save_active_bindings(..., joystick_labels, joystick_repeat)`, and `JoyHeld`/`joystick_held` are consistent across tasks.
