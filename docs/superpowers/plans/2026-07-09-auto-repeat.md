# Auto-repeat (typematic) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Held hold-means-hold key bindings auto-repeat like a physical keyboard (initial delay, then a steady rate), configurable globally (timing) and per-binding (on/off).

**Architecture:** A periodic `tick(now)` is driven from the existing consumer loops into the dispatcher; the dispatcher re-fires held, repeat-enabled keys on schedule via the existing `injector.key_down`. Timing is app-global (manifest `[autorepeat]`); enable is per-binding (profile `[repeat]` table). The dispatcher stays single-threaded — no new thread, no locks. All repeat state lives in the existing `held_keys`, so release/dry-run/disconnect/shutdown already stop repeats.

**Tech Stack:** Rust (GNU toolchain), `std::time::{Instant, Duration}`, `toml`/`serde`, egui/eframe, Win32 `SendInput` (unchanged).

Full design: `docs/superpowers/specs/2026-07-09-auto-repeat-design.md`.

## Global Constraints

- Build with the **GNU** toolchain; if `cargo`/`gcc` are missing from PATH, prepend: `export PATH="$HOME/.cargo/bin:/c/Strawberry/c/bin:$PATH"`. Do **not** switch to the MSVC target.
- This is a **binary** crate: run `cargo test` (NOT `cargo test --lib`, which fails).
- **TDD** for pure-logic modules (`config.rs`, `dispatcher.rs`): write the failing test first, confirm it fails, then implement. Consumer-loop wiring (`runtime.rs`, `monitor/mod.rs` `consumer_loop`) and GUI widgets are the documented **manual-verify** exception (no unit tests).
- **No `panic!`/`unwrap()` in the runtime path.** Injection/repeat failures log a warning and continue.
- **Platform isolation:** no Win32 types in `dispatcher`/`config`/`protocol`; OS code stays behind `#[cfg(...)]` in `src/injector/`. (This feature reuses the existing `injector.key_down` — no new injector code.)
- **Timing defaults:** `delay_ms = 400`, `interval_ms = 40`. `interval_ms` is clamped to a floor of `1` on load (a `0` interval would busy-spin). `delay_ms` may be `0`.
- **Per-binding default is off:** a key absent from `[repeat]` (or present and `false`) does not repeat.
- **Repeat re-fires the combo's key only** (modifiers stay held from `combo_down`); modifier-only combos never repeat.
- Backward compatibility: an old profile with no `[repeat]` and a manifest with no `[autorepeat]` behave exactly as today.
- One focused commit per task; imperative subject; end each commit message with:
  `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`

---

### Task 1: Config — global `[autorepeat]` timing

**Files:**
- Modify: `src/config.rs` (add `AutoRepeat`/`RawAutoRepeat`, extend `RawManifest`, add `ProfileSet.autorepeat` field + accessor; tests in the `profileset_tests` module)

**Interfaces:**
- Produces: `pub struct AutoRepeat { pub delay_ms: u64, pub interval_ms: u64 }` (derives `Debug, Clone, Copy, PartialEq, Eq`); `impl Default for AutoRepeat` → `{ 400, 40 }`; `ProfileSet::autorepeat(&self) -> AutoRepeat`.
- Consumes: nothing new.

- [ ] **Step 1: Write the failing tests**

Add to the `profileset_tests` module in `src/config.rs` (it already has `write`, `tmp`, and `use super::*;`):

```rust
    #[test]
    fn autorepeat_defaults_when_absent() {
        let d = tmp("ar-default");
        write(&d, "config.toml", "[keys]\nG1 = \"a\"\n");
        let set = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert_eq!(set.autorepeat(), AutoRepeat { delay_ms: 400, interval_ms: 40 });
    }

    #[test]
    fn autorepeat_parses_values() {
        let d = tmp("ar-parse");
        write(&d.join("profiles"), "default.toml", "[keys]\nG1 = \"a\"\n");
        write(&d, "config.toml",
            "profiles_dir = \"profiles\"\nm1 = \"default.toml\"\n[autorepeat]\ndelay_ms = 250\ninterval_ms = 33\n");
        let set = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert_eq!(set.autorepeat(), AutoRepeat { delay_ms: 250, interval_ms: 33 });
    }

    #[test]
    fn autorepeat_interval_zero_clamped_to_one() {
        let d = tmp("ar-clamp");
        write(&d, "config.toml", "[keys]\nG1 = \"a\"\n[autorepeat]\ninterval_ms = 0\n");
        let set = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert_eq!(set.autorepeat().interval_ms, 1);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test autorepeat`
Expected: FAIL — `no method named 'autorepeat'` / `cannot find type 'AutoRepeat'`.

- [ ] **Step 3: Implement**

In `src/config.rs`, add the types after `fn default_deadzone()` (near line 28):

```rust
#[derive(Debug, Deserialize, Clone)]
struct RawAutoRepeat {
    #[serde(default = "default_delay_ms")]
    delay_ms: u64,
    #[serde(default = "default_interval_ms")]
    interval_ms: u64,
}

fn default_delay_ms() -> u64 { 400 }
fn default_interval_ms() -> u64 { 40 }

/// Global auto-repeat timing (from the manifest `[autorepeat]`; defaults when absent).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutoRepeat {
    pub delay_ms: u64,
    pub interval_ms: u64,
}

impl Default for AutoRepeat {
    fn default() -> Self { Self { delay_ms: 400, interval_ms: 40 } }
}

impl AutoRepeat {
    fn from_raw(r: RawAutoRepeat) -> Self {
        Self {
            delay_ms: r.delay_ms,
            interval_ms: r.interval_ms.max(1), // 0 would busy-spin the tick
        }
    }
}
```

Extend `RawManifest` (around line 115) with a new field:

```rust
#[derive(Debug, Deserialize)]
struct RawManifest {
    profiles_dir: Option<String>,
    m1: Option<String>,
    m2: Option<String>,
    m3: Option<String>,
    #[serde(default)]
    autorepeat: Option<RawAutoRepeat>,
}
```

Add the field to `ProfileSet` (after `active: MKey,` around line 136):

```rust
    active: MKey,
    autorepeat: AutoRepeat,
```

In `ProfileSet::load`, compute the value once right after `let base = ...` (before `if let Some(m1_name) = raw.m1`):

```rust
        let autorepeat = raw.autorepeat.map(AutoRepeat::from_raw).unwrap_or_default();
```

Then add `autorepeat,` to **both** `Ok(Self { ... })` constructors (manifest mode and legacy mode).

Add the accessor near `active()` (around line 186):

```rust
    pub fn autorepeat(&self) -> AutoRepeat { self.autorepeat }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test autorepeat`
Expected: PASS (3 tests). Also run `cargo test` — all existing tests still pass.

- [ ] **Step 5: Commit**

```bash
git add src/config.rs
git commit -m "feat: parse global [autorepeat] timing from the manifest"
```

---

### Task 2: Config — per-binding `[repeat]` table + save

**Files:**
- Modify: `src/config.rs` (add `repeat` to `RawConfig` + `Profile`; `repeats`/`set_repeat`; extend `to_toml`; change `save_active_bindings` signature; update the `raw()` test helper and the existing save test; tests in the `tests` module)

**Interfaces:**
- Produces: `Profile::repeats(&self, key: G13Key) -> bool` (default `false`); `Profile::set_repeat(&mut self, repeat: HashMap<G13Key, bool>)`; `ProfileSet::save_active_bindings(&mut self, bindings: HashMap<G13Key, String>, repeat: HashMap<G13Key, bool>) -> Result<()>`.
- Consumes: `AutoRepeat` from Task 1 (same file; no direct use here).

- [ ] **Step 1: Write the failing tests**

First update the `raw()` helper in the `tests` module (around line 424) so it compiles with the new field:

```rust
    fn raw(pairs: &[(&str, &str)]) -> RawConfig {
        RawConfig {
            keys: pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            joystick: None,
            repeat: HashMap::new(),
        }
    }
```

Add these tests to the `tests` module in `src/config.rs`:

```rust
    #[test]
    fn parses_repeat_flags() {
        let src = "[keys]\nG1 = \"a\"\nG2 = \"b\"\n[repeat]\nG2 = true\n";
        let raw: RawConfig = toml::from_str(src).unwrap();
        let p = Profile::from_raw(raw).unwrap();
        assert!(!p.repeats(G13Key::G1));
        assert!(p.repeats(G13Key::G2));
    }

    #[test]
    fn repeat_defaults_false_when_absent() {
        let p = Profile::from_raw(raw(&[("G1", "a")])).unwrap();
        assert!(!p.repeats(G13Key::G1));
    }

    #[test]
    fn repeat_round_trips_through_toml() {
        let src = "[keys]\nG1 = \"a\"\nG2 = \"b\"\n[repeat]\nG2 = true\n";
        let raw: RawConfig = toml::from_str(src).unwrap();
        let p = Profile::from_raw(raw).unwrap();
        let toml = p.to_toml().unwrap();
        let reloaded = Profile::from_raw(toml::from_str(&toml).unwrap()).unwrap();
        assert!(reloaded.repeats(G13Key::G2));
        assert!(!reloaded.repeats(G13Key::G1));
    }

    #[test]
    fn to_toml_omits_disabled_repeat_flags() {
        use std::collections::HashMap;
        let mut map = HashMap::new();
        map.insert(G13Key::G1, false);
        let mut p = Profile::from_raw(raw(&[("G1", "a")])).unwrap();
        p.set_repeat(map);
        let toml = p.to_toml().unwrap();
        assert!(!toml.contains("[repeat]"));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test repeat`
Expected: FAIL — `no field 'repeat' on RawConfig` / `no method named 'repeats'`.

- [ ] **Step 3: Implement**

Add the field to `RawConfig` (around line 7):

```rust
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RawConfig {
    #[serde(default)]
    pub keys: HashMap<String, String>,
    #[serde(default)]
    pub joystick: Option<RawJoystick>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub repeat: HashMap<String, bool>,
}
```

Add the field to `Profile` (around line 46):

```rust
#[derive(Debug, Clone)]
pub struct Profile {
    key_bindings: HashMap<G13Key, String>,
    joystick: Option<JoystickConfig>,
    repeat: HashMap<G13Key, bool>,
}
```

In `Profile::from_raw`, after building `key_bindings` and before the `joystick` block, parse `[repeat]`; then include `repeat` in the returned struct:

```rust
        let mut repeat = HashMap::new();
        for (name, on) in raw.repeat {
            let key = parse_g13_key(&name)
                .with_context(|| format!("unknown G13 key in [repeat]: {}", name))?;
            repeat.insert(key, on);
        }

        let joystick = match raw.joystick {
            Some(rj) => Some(parse_joystick(rj)?),
            None => None,
        };

        Ok(Self { key_bindings, joystick, repeat })
```

Add the accessors after `set_bindings` (around line 91):

```rust
    pub fn repeats(&self, key: G13Key) -> bool {
        *self.repeat.get(&key).unwrap_or(&false)
    }

    pub fn set_repeat(&mut self, repeat: HashMap<G13Key, bool>) {
        self.repeat = repeat;
    }
```

In `to_toml`, build the repeat map (only enabled keys) and pass it to `RawConfig`:

```rust
        let repeat: HashMap<String, bool> = self.repeat.iter()
            .filter(|(_, &v)| v)
            .map(|(k, _)| (format!("{k:?}"), true)) // Debug of G13Key: "G1".."Stick"
            .collect();
        let raw = RawConfig { keys, joystick, repeat };
        toml::to_string(&raw).context("failed to serialize profile")
```

Change `save_active_bindings` to also set the repeat map (around line 257):

```rust
    pub fn save_active_bindings(
        &mut self,
        bindings: HashMap<G13Key, String>,
        repeat: HashMap<G13Key, bool>,
    ) -> Result<()> {
        let path = self.active_path();
        let profile = self.active_profile_mut();
        profile.set_bindings(bindings);
        profile.set_repeat(repeat);
        let toml = profile.to_toml()?;
        std::fs::write(&path, toml)
            .with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }
```

Update the existing test `save_active_bindings_writes_and_preserves_others` (around line 403) to pass an empty repeat map:

```rust
        set.save_active_bindings(b, HashMap::new()).unwrap();
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test repeat` then `cargo test`
Expected: PASS — the 4 new tests plus all existing tests (the updated save test still passes).

- [ ] **Step 5: Commit**

```bash
git add src/config.rs
git commit -m "feat: per-binding [repeat] flags with save/round-trip"
```

---

### Task 3: Dispatcher — held-key repeat schedule + `tick`

**Files:**
- Modify: `src/dispatcher.rs` (change `held_keys` value type to `HeldKey`; record repeat+timing in `handle_key_down`; add `tick`; update `handle_key_up`/`release_held`; add a repeat-config test helper + tests)

**Interfaces:**
- Consumes: `ProfileSet::autorepeat()` (Task 1); `Profile::repeats()` (Task 2); the existing `KeyInjector::key_down(&str)`, `combo_down`, `combo_up`, `press`.
- Produces: `Dispatcher::tick(&mut self, now: Instant)`.

- [ ] **Step 1: Write the failing tests**

Add to the top of the `tests` module in `src/dispatcher.rs` (below the existing `use` lines):

```rust
    use std::time::{Duration, Instant};

    fn make_config_repeat(keys: &str, repeat: &str, delay: u64, interval: u64) -> Arc<RwLock<ProfileSet>> {
        let d = tmp("rep");
        std::fs::create_dir_all(&d).unwrap();
        let body = format!(
            "[keys]\n{keys}\n[repeat]\n{repeat}\n[autorepeat]\ndelay_ms = {delay}\ninterval_ms = {interval}\n"
        );
        write(&d.join("config.toml"), &body);
        Arc::new(RwLock::new(ProfileSet::load(&d.join("config.toml")).unwrap()))
    }
```

Then add these tests to the same module:

```rust
    #[test]
    fn held_key_repeats_after_delay() {
        let (injector, holds) = MockInjector::new_with_holds();
        let config = make_config_repeat("G1 = \"a\"", "G1 = true", 100, 50);
        let mut d = Dispatcher::new(config, Box::new(injector));
        let t0 = Instant::now();
        d.handle(G13Event::KeyDown(G13Key::G1)).unwrap();
        d.tick(t0); // schedules first repeat at t0+100ms; no fire yet
        assert!(holds.lock().unwrap().is_empty());
        d.tick(t0 + Duration::from_millis(101)); // first repeat
        assert_eq!(*holds.lock().unwrap(), vec!["down:a".to_string()]);
        d.tick(t0 + Duration::from_millis(151)); // second repeat
        assert_eq!(*holds.lock().unwrap(),
            vec!["down:a".to_string(), "down:a".to_string()]);
    }

    #[test]
    fn disabled_key_never_repeats() {
        let (injector, holds) = MockInjector::new_with_holds();
        // G1 bound but only G2 is in [repeat].
        let config = make_config_repeat("G1 = \"a\"", "G2 = true", 100, 50);
        let mut d = Dispatcher::new(config, Box::new(injector));
        let t0 = Instant::now();
        d.handle(G13Event::KeyDown(G13Key::G1)).unwrap();
        d.tick(t0);
        d.tick(t0 + Duration::from_millis(500));
        assert!(holds.lock().unwrap().is_empty());
    }

    #[test]
    fn combo_repeat_fires_key_only() {
        let (injector, holds) = MockInjector::new_with_holds();
        let config = make_config_repeat("G1 = \"ctrl+c\"", "G1 = true", 100, 50);
        let mut d = Dispatcher::new(config, Box::new(injector));
        let t0 = Instant::now();
        d.handle(G13Event::KeyDown(G13Key::G1)).unwrap();
        d.tick(t0 + Duration::from_millis(101));
        // combo_down recorded elsewhere; the repeat re-fires only the key "c".
        assert_eq!(*holds.lock().unwrap(), vec!["down:c".to_string()]);
    }

    #[test]
    fn modifier_only_repeat_is_noop() {
        let (injector, holds) = MockInjector::new_with_holds();
        let config = make_config_repeat("G1 = \"shift\"", "G1 = true", 100, 50);
        let mut d = Dispatcher::new(config, Box::new(injector));
        let t0 = Instant::now();
        d.handle(G13Event::KeyDown(G13Key::G1)).unwrap();
        d.tick(t0 + Duration::from_millis(300));
        assert!(holds.lock().unwrap().is_empty(), "modifier-only has no key to repeat");
    }

    #[test]
    fn media_key_with_repeat_never_held_or_repeated() {
        let (injector, holds) = MockInjector::new_with_holds();
        let config = make_config_repeat("G1 = \"playpause\"", "G1 = true", 100, 50);
        let mut d = Dispatcher::new(config, Box::new(injector));
        let t0 = Instant::now();
        d.handle(G13Event::KeyDown(G13Key::G1)).unwrap();
        d.tick(t0 + Duration::from_millis(300));
        assert!(holds.lock().unwrap().is_empty()); // tapped via press(), never held
    }

    #[test]
    fn repeat_stops_after_key_up() {
        let (injector, holds) = MockInjector::new_with_holds();
        let config = make_config_repeat("G1 = \"a\"", "G1 = true", 100, 50);
        let mut d = Dispatcher::new(config, Box::new(injector));
        let t0 = Instant::now();
        d.handle(G13Event::KeyDown(G13Key::G1)).unwrap();
        d.tick(t0 + Duration::from_millis(101)); // one repeat
        d.handle(G13Event::KeyUp(G13Key::G1)).unwrap();
        holds.lock().unwrap().clear();
        d.tick(t0 + Duration::from_millis(300)); // released -> no more
        assert!(holds.lock().unwrap().is_empty());
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test repeat` (or `cargo test held_key_repeats`)
Expected: FAIL — `no method named 'tick'`.

- [ ] **Step 3: Implement**

In `src/dispatcher.rs`, add to the imports at the top:

```rust
use std::time::{Duration, Instant};
```

Add the `HeldKey` struct just above `pub struct Dispatcher` (around line 10):

```rust
/// A currently-held key binding plus its auto-repeat schedule.
struct HeldKey {
    combo: KeyCombo,
    repeat: bool,        // snapshot of profile.repeats(key) at press time
    delay_ms: u64,       // snapshot of manifest timing at press time
    interval_ms: u64,
    next_repeat: Option<Instant>, // None until the first tick schedules it
}
```

Change the `held_keys` field type:

```rust
    held_keys: HashMap<G13Key, HeldKey>,
```

Replace `handle_key_down` with a version that reads repeat + timing under one lock and stores them:

```rust
    fn handle_key_down(&mut self, key: G13Key) {
        let (binding, repeat, ar) = {
            let set = self.profiles.read().unwrap();
            let p = set.active_profile();
            (p.get_binding(key).map(str::to_owned), p.repeats(key), set.autorepeat())
        };
        let Some(binding) = binding else {
            log::debug!("{key:?} -> (unmapped)");
            return;
        };
        log::debug!("{key:?} -> {binding}");
        let combo = match KeyCombo::parse(&binding) {
            Ok(c) => c,
            Err(e) => { log::warn!("bad binding {binding:?}: {e:#}"); return; }
        };
        // Media keys tap; everything else holds.
        let is_media = combo.key.as_ref().is_some_and(|k| self.tap_only.contains(k));
        if is_media {
            if let Err(e) = self.injector.press(&combo) {
                log::warn!("injection failed: {e:#}");
            }
        } else {
            match self.injector.combo_down(&combo) {
                Ok(()) => {
                    self.held_keys.insert(key, HeldKey {
                        combo,
                        repeat,
                        delay_ms: ar.delay_ms,
                        interval_ms: ar.interval_ms,
                        next_repeat: None,
                    });
                }
                Err(e) => log::warn!("injection failed: {e:#}"),
            }
        }
    }
```

Replace `handle_key_up` to read `held.combo`:

```rust
    fn handle_key_up(&mut self, key: G13Key) {
        if let Some(held) = self.held_keys.remove(&key) {
            if let Err(e) = self.injector.combo_up(&held.combo) {
                log::warn!("injection failed: {e:#}");
            }
        }
    }
```

Add the `tick` method (place it right after `handle_key_up`):

```rust
    /// Re-fire held, repeat-enabled keys whose interval has elapsed. Called
    /// periodically by the consumer loop with the current time. Collect first,
    /// inject second, so we don't borrow `held_keys` while calling the injector.
    pub fn tick(&mut self, now: Instant) {
        let mut to_fire: Vec<String> = Vec::new();
        for held in self.held_keys.values_mut() {
            if !held.repeat { continue; }
            let Some(key) = held.combo.key.as_deref() else { continue; };
            match held.next_repeat {
                None => {
                    held.next_repeat = Some(now + Duration::from_millis(held.delay_ms));
                }
                Some(mut due) => {
                    while now >= due {
                        to_fire.push(key.to_string());
                        due += Duration::from_millis(held.interval_ms);
                    }
                    held.next_repeat = Some(due);
                }
            }
        }
        for key in to_fire {
            if let Err(e) = self.injector.key_down(&key) {
                log::warn!("auto-repeat injection failed: {e:#}");
            }
        }
    }
```

Update `release_held` to drain `HeldKey` values:

```rust
    pub fn release_held(&mut self) {
        self.release_joystick();
        for (_key, held) in self.held_keys.drain() {
            if let Err(e) = self.injector.combo_up(&held.combo) {
                log::warn!("injection failed on release: {e:#}");
            }
        }
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test` (runs the whole binary's tests)
Expected: PASS — the 6 new dispatcher tests plus all existing tests (the migrated `held_keys` type does not change existing behavior).

- [ ] **Step 5: Commit**

```bash
git add src/dispatcher.rs
git commit -m "feat: auto-repeat schedule and tick() in the dispatcher"
```

---

### Task 4: Wire `tick` into the consumer loops

**Files:**
- Modify: `src/runtime.rs` (`run_headless` becomes a `recv_timeout` loop that ticks)
- Modify: `src/monitor/mod.rs` (`consumer_loop`: shorten the timeout to 15ms and tick on both branches)

**Interfaces:**
- Consumes: `Dispatcher::tick(Instant)` (Task 3).
- Produces: nothing (loop wiring).

This task is the documented **manual-verify** exception (event-loop/IO wiring — no unit test). Verification is a clean build, the existing suite still green, and the Task 6 smoke test.

- [ ] **Step 1: Edit `run_headless` in `src/runtime.rs`**

Change the module imports at the top of `src/runtime.rs`:

```rust
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};
```

Replace the event loop in `run_headless` (the `for event in rx { ... }` block and the `release_held()` after it) with:

```rust
    loop {
        match rx.recv_timeout(Duration::from_millis(15)) {
            Ok(event) => {
                if let Err(e) = dispatcher.handle(event) {
                    log::warn!("dispatch error: {e:#}");
                }
                dispatcher.tick(Instant::now());
            }
            Err(RecvTimeoutError::Timeout) => dispatcher.tick(Instant::now()),
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    dispatcher.release_held();
    Ok(())
```

- [ ] **Step 2: Edit `consumer_loop` in `src/monitor/mod.rs`**

Change the time import at the top of `src/monitor/mod.rs`:

```rust
use std::time::{Duration, Instant};
```

In `consumer_loop`, change the timeout from `50` to `15`:

```rust
        match rx.recv_timeout(Duration::from_millis(15)) {
```

In the `Ok(event)` arm, add a tick just before `ctx.request_repaint();`:

```rust
                was_active = active;
                dispatcher.tick(Instant::now());
                ctx.request_repaint();
```

In the `Err(RecvTimeoutError::Timeout)` arm, add a tick after `was_active = active;`:

```rust
                was_active = active;
                dispatcher.tick(Instant::now());
```

(The `Disconnected` arm is unchanged.)

- [ ] **Step 3: Build and run the existing suite**

Run: `cargo build && cargo test`
Expected: clean build (only the pre-existing `usb.rs` `mut self` warning) and all tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/runtime.rs src/monitor/mod.rs
git commit -m "feat: drive dispatcher tick() from the consumer loops"
```

---

### Task 5: GUI — per-binding repeat checkbox

**Files:**
- Modify: `src/monitor/mod.rs` (`render_binding_row` gains a checkbox; `MonitorApp` gains `repeat_edits`; reload/save handle it; a caption points to `[autorepeat]`)

**Interfaces:**
- Consumes: `Profile::repeats()` and the two-arg `ProfileSet::save_active_bindings(bindings, repeat)` (Task 2).
- Produces: nothing (GUI).

Documented **manual-verify** exception (GUI widget). Verification is a clean build and a visual check.

- [ ] **Step 1: Extend `render_binding_row`**

Replace the `render_binding_row` function (around lines 27-50) with:

```rust
fn render_binding_row(
    ui: &mut egui::Ui,
    key: G13Key,
    edits: &mut HashMap<G13Key, String>,
    repeat_edits: &mut HashMap<G13Key, bool>,
    valid_keys: &HashSet<String>,
) {
    let green = egui::Color32::from_rgb(127, 224, 160);
    let red = egui::Color32::from_rgb(220, 90, 90);
    let dim = egui::Color32::from_gray(110);
    let buf = edits.entry(key).or_default();
    let rep = repeat_edits.entry(key).or_default();
    ui.horizontal(|ui| {
        ui.monospace(format!("{key:?}"));
        ui.add_space(6.0);
        ui.add(egui::TextEdit::singleline(buf).desired_width(160.0));
        let (mark, color) = if buf.is_empty() {
            ("—", dim)
        } else if combo_valid(buf, valid_keys) {
            ("ok", green)
        } else {
            ("bad", red)
        };
        ui.colored_label(color, mark);
        ui.add_space(6.0);
        ui.checkbox(rep, "repeat");
    });
}
```

- [ ] **Step 2: Add the `repeat_edits` field to `MonitorApp`**

In the `MonitorApp` struct (around line 92) add the field after `edits`:

```rust
    edits: HashMap<G13Key, String>,
    repeat_edits: HashMap<G13Key, bool>,
```

In `MonitorApp::new` (around line 109) initialize it after `edits: HashMap::new(),`:

```rust
            edits: HashMap::new(),
            repeat_edits: HashMap::new(),
```

- [ ] **Step 3: Build both edit buffers on reload and use the new row signature**

In `render_bindings`, inside the `if self.edits_for != active_name { ... }` reload block, add the repeat buffer right after the `self.edits = ...` assignment (before `drop(set);`):

```rust
            self.repeat_edits = ROWS.iter().flat_map(|row| row.iter()).chain(THUMB.iter())
                .map(|&k| (k, profile.repeats(k)))
                .collect();
```

Update both `render_binding_row` call sites (the `ROWS` loop and the `THUMB` loop) to pass `&mut self.repeat_edits`:

```rust
                render_binding_row(ui, key, &mut self.edits, &mut self.repeat_edits, &valid_keys);
```

- [ ] **Step 4: Collect repeat flags on Save + add a caption**

In the Save button handler, build the repeat map and pass it to the two-arg save:

```rust
            if ui.add_enabled(all_valid, egui::Button::new("Save")).clicked() {
                let bindings: HashMap<G13Key, String> = self.edits.iter()
                    .filter(|(_, v)| !v.is_empty())
                    .map(|(k, v)| (*k, v.clone()))
                    .collect();
                let repeat: HashMap<G13Key, bool> = self.repeat_edits.iter()
                    .filter(|(_, &v)| v)
                    .map(|(k, &v)| (*k, v))
                    .collect();
                match self.profiles.write().unwrap().save_active_bindings(bindings, repeat) {
                    Ok(()) => self.save_status = Some("saved".to_string()),
                    Err(e) => {
                        log::warn!("save failed: {e:#}");
                        self.save_status = Some(format!("save failed: {e:#}"));
                    }
                }
            }
```

Add a caption line just after the existing `ui.weak("Combo = ...")` help text (before `ui.add_space(6.0);`):

```rust
        ui.weak("Tick 'repeat' to auto-repeat a key while held (like a keyboard). Repeat \
                 timing (delay/rate) is set in config.toml under [autorepeat].");
```

- [ ] **Step 5: Build, run, and visually verify**

Run:
```bash
cargo build --release
./target/release/g13-driver.exe
```
Expected: the Bindings tab shows a `repeat` checkbox on every key row (G-keys + thumb buttons); toggling one and clicking **Save** writes `[repeat]` to the active profile file (open it to confirm); **Revert** restores the saved state. Close the app.

- [ ] **Step 6: Commit**

```bash
git add src/monitor/mod.rs
git commit -m "feat: per-binding repeat checkbox in the Bindings editor"
```

---

### Task 6: Hardware smoke test, example config, and milestone

**Files:**
- Modify: `config.toml` (add a commented `[autorepeat]` example)
- Create: `milestones/finished/auto-repeat.md`

**Interfaces:** none.

- [ ] **Step 1: Add a commented `[autorepeat]` example to `config.toml`**

Append to `config.toml` (the manifest) a documented, commented-out block so the timing knob is discoverable (leave it commented so defaults apply unless the user opts in):

```toml

# Auto-repeat timing (global). Uncomment to override the defaults below.
# Per-binding on/off lives in each profile's [repeat] table (Bindings tab checkbox).
# [autorepeat]
# delay_ms = 400      # wait before repeating starts
# interval_ms = 40    # gap between repeats (~25/sec); min 1
```

- [ ] **Step 2: Build the release binary and run the smoke test**

Run:
```bash
cargo build --release
./target/release/g13-driver.exe
```

Guide the user (needs the G13 + a physical keyboard reference):
1. **Bindings tab:** on the default profile, set `G1 = a` with **repeat ticked**, `G2 = b` with **repeat unticked**; **Save**.
2. Switch to **Active**. In Notepad, **hold G1** → after the delay, `a` repeats at a steady rate (`aaaa…`); **hold G2** → types a single `b` and holds (no repeat).
3. Release G1 → repeating stops immediately, no stuck key.
4. Optional: edit `config.toml` `[autorepeat]` (e.g. `delay_ms = 200`, `interval_ms = 25`), save → hot-reload picks it up; a freshly pressed-and-held key uses the new timing.
5. Optional (game): hold a repeating movement key in a game → the held key still registers normally.

- [ ] **Step 3: Write the milestone**

Create `milestones/finished/auto-repeat.md`:

```markdown
# Auto-repeat (typematic) for held keys

- **Status:** finished
- **Date:** <fill in on completion>

## Outcome
Held hold-means-hold bindings auto-repeat like a physical keyboard. Spec:
`docs/superpowers/specs/2026-07-09-auto-repeat-design.md`; plan:
`docs/superpowers/plans/2026-07-09-auto-repeat.md`.
- Windows does not auto-repeat injected keys (typematic is tied to the physical device), so the
  driver re-injects: a ~15ms `tick` from the consumer loops calls `Dispatcher::tick`, which
  re-fires held, repeat-enabled keys after an initial delay at a steady interval.
- Global timing in the manifest `[autorepeat]` (`delay_ms`/`interval_ms`, defaults 400/40,
  interval clamped to >=1); per-binding opt-in via each profile's `[repeat]` table, edited with a
  checkbox in the Bindings tab. Repeat re-fires the combo's key only (modifiers stay held);
  modifier-only and joystick never repeat; media keys still tap.
- No new thread/locks — repeat state lives in `held_keys`, so release/dry-run/disconnect/shutdown
  stop repeats for free.

## Follow-ups
- GUI editor for the global `[autorepeat]` timing (planned Settings tab; edited in config.toml for now).
- Per-binding timing overrides (timing is global only).
```

- [ ] **Step 4: Commit**

```bash
git add config.toml milestones/finished/auto-repeat.md
git commit -m "docs: auto-repeat example config and milestone"
```

---

## Notes for the executor

- After all tasks: run the final whole-branch review (most capable model), then use
  `superpowers:finishing-a-development-branch`.
- If the GUI clobbered `profiles/default.toml` during a smoke test (comments stripped / keys
  reordered), restore it with `git checkout -- profiles/default.toml` before finishing — that is
  test debris, not a real change (known follow-up: comment-preserving + deterministic saves).
