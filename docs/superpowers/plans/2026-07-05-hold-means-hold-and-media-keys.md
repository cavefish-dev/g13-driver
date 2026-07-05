# Hold-means-hold G-keys + Multimedia Keys Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make G-key bindings hold-means-hold (a G-key holds its bound key/combo while held, releasing on release), enabling chording / held modifiers / held movement keys; add multimedia keys as the tap-only exception.

**Architecture:** `KeyCombo.key` becomes `Option<String>` (modifier-only combos). The injector gains `combo_down`/`combo_up` (hold/release a combo); `press` (tap) stays for tap-only media keys. The dispatcher tracks `held_keys` per G13Key: KeyDown holds (or taps a media key), KeyUp releases; `release_held` lifts held G-keys + the joystick on Dry-run/disconnect/shutdown.

**Tech Stack:** Rust, GNU toolchain (`stable-x86_64-pc-windows-gnu`), `windows-sys` (`SendInput`), `eframe`/`egui`, `toml`/`serde`, `log`. Build/test: `cargo` (PATH may need `export PATH="$HOME/.cargo/bin:/c/Strawberry/c/bin:$PATH"`).

## Global Constraints

- **Windows-only** (`src/main.rs:1-2`). `SendInput` code (`injector/windows.rs`) stays behind `#[cfg(windows)]` and has NO unit tests (manual-verify). Pure logic (parser, key_map, dispatcher) is TDD.
- **Hold-means-hold:** KeyDown holds the bound combo, KeyUp releases it. **Multimedia keys are tap-only** (a media binding taps on KeyDown, not tracked). Everything else holds.
- **`KeyCombo.key: Option<String>`** — modifier-only combos are valid (`shift`, `ctrl+shift`); empty combos (no key, no modifier) and multi-key combos error.
- **Media key names + VKs:** `playpause` 0xB3, `nexttrack`/`next` 0xB0, `prevtrack`/`prev` 0xB1, `mediastop` 0xB2, `volup`/`volumeup` 0xAF, `voldown`/`volumedown` 0xAE, `mute` 0xAD.
- **Release paths:** `release_held()` (Active→Dry-run, disconnect, shutdown) releases held G-keys AND the joystick. **Profile switch** releases only the joystick (held G-keys stay until their physical KeyUp).
- **Error policy:** injection failures `log::warn!` and continue; no `panic!`/`unwrap()` in the runtime path (test code may `unwrap`; `mutex/rwlock.unwrap()` is the accepted poison-unreachable exception).
- **Commits:** one per task; imperative subject; end with the trailer `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.
- Binary crate — `cargo test` (not `--lib`); focused: `cargo test <module>::`.

---

## File Structure

| File | Change | Responsibility |
|------|--------|----------------|
| `src/injector/key_map.rs` | Modify | Add media keys; `tap_only_keys()` |
| `src/injector/mod.rs` | Modify | `KeyCombo.key: Option<String>`; parser; `combo_down`/`combo_up` trait methods |
| `src/injector/windows.rs` | Modify | `press` handles Option key; implement `combo_down`/`combo_up` |
| `src/monitor/mod.rs` | Modify | `combo_valid` handles Option key + media; hints line |
| `src/dispatcher.rs` | Modify | Hold-means-hold: `held_keys`, tap-only, KeyDown/KeyUp, release split |

---

## Task 1: Multimedia keys + tap-only set

**Files:** Modify `src/injector/key_map.rs`.

**Interfaces:**
- Produces: media entries in `build_key_map()`; `pub fn tap_only_keys() -> HashSet<String>`.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `src/injector/key_map.rs`:

```rust
    #[test]
    fn media_keys_present() {
        let m = build_key_map();
        assert_eq!(m["playpause"], 0xB3);
        assert_eq!(m["nexttrack"], 0xB0);
        assert_eq!(m["next"], 0xB0);
        assert_eq!(m["prevtrack"], 0xB1);
        assert_eq!(m["volup"], 0xAF);
        assert_eq!(m["voldown"], 0xAE);
        assert_eq!(m["mute"], 0xAD);
        assert_eq!(m["mediastop"], 0xB2);
    }

    #[test]
    fn tap_only_is_media_only() {
        let t = tap_only_keys();
        assert!(t.contains("playpause"));
        assert!(t.contains("volup"));
        assert!(t.contains("mute"));
        assert!(!t.contains("a"));
        assert!(!t.contains("shift"));
        assert!(!t.contains("f5"));
    }
```

Also add `use std::collections::HashSet;` to the top of `key_map.rs` (next to the existing `use std::collections::HashMap;`).

- [ ] **Step 2: Run to verify failure**

Run: `cargo test key_map:: 2>&1 | tail -12`
Expected: FAIL — `m["playpause"]` panics (missing) / `tap_only_keys` not found.

- [ ] **Step 3: Implement**

In `src/injector/key_map.rs`, add the media entries to the `specials` slice (append these lines inside the `&[...]`):

```rust
        // Multimedia keys (tap-only).
        ("playpause",   0xB3),
        ("nexttrack",   0xB0), ("next",         0xB0),
        ("prevtrack",   0xB1), ("prev",         0xB1),
        ("mediastop",   0xB2),
        ("volup",       0xAF), ("volumeup",     0xAF),
        ("voldown",     0xAE), ("volumedown",   0xAE),
        ("mute",        0xAD),
```

Add the function after `build_key_map`:

```rust
/// Names of keys that should tap (down+up on press) rather than hold — the
/// multimedia keys, where holding is meaningless. Everything else holds.
pub fn tap_only_keys() -> HashSet<String> {
    ["playpause", "nexttrack", "next", "prevtrack", "prev", "mediastop",
     "volup", "volumeup", "voldown", "volumedown", "mute"]
        .iter().map(|s| s.to_string()).collect()
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test key_map:: 2>&1 | tail -5`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/injector/key_map.rs
git commit -m "feat: add multimedia keys and a tap-only key set

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: KeyCombo.key → Option<String> (modifier-only combos)

**Files:** Modify `src/injector/mod.rs`, `src/injector/windows.rs`, `src/monitor/mod.rs`.

**Interfaces:**
- Produces: `KeyCombo { modifiers: Vec<Modifier>, key: Option<String> }`; `parse` accepts modifier-only combos.

- [ ] **Step 1: Update the parser tests (RED)**

In `src/injector/mod.rs` test module: change the `c.key` assertions to `Option`, change `parse_no_key_is_error` to expect success (modifier-only), and add an empty-combo error test:

```rust
    #[test]
    fn parse_single_key() {
        let c = KeyCombo::parse("f5").unwrap();
        assert_eq!(c.key.as_deref(), Some("f5"));
        assert!(c.modifiers.is_empty());
    }

    #[test]
    fn parse_ctrl_c() {
        let c = KeyCombo::parse("ctrl+c").unwrap();
        assert_eq!(c.key.as_deref(), Some("c"));
        assert_eq!(c.modifiers, vec![Modifier::Ctrl]);
    }

    #[test]
    fn parse_shift_ctrl_esc() {
        let c = KeyCombo::parse("shift+ctrl+esc").unwrap();
        assert_eq!(c.key.as_deref(), Some("esc"));
        assert!(c.modifiers.contains(&Modifier::Ctrl));
        assert!(c.modifiers.contains(&Modifier::Shift));
    }

    #[test]
    fn parse_is_case_insensitive() {
        let c = KeyCombo::parse("CTRL+C").unwrap();
        assert_eq!(c.key.as_deref(), Some("c"));
        assert_eq!(c.modifiers, vec![Modifier::Ctrl]);
    }

    #[test]
    fn parse_windows_key() {
        let c = KeyCombo::parse("windows+d").unwrap();
        assert_eq!(c.key.as_deref(), Some("d"));
        assert_eq!(c.modifiers, vec![Modifier::Windows]);
    }

    #[test]
    fn parse_modifier_only_is_ok() {
        let c = KeyCombo::parse("ctrl+shift").unwrap();
        assert!(c.key.is_none());
        assert_eq!(c.modifiers, vec![Modifier::Ctrl, Modifier::Shift]);
        let c = KeyCombo::parse("shift").unwrap();
        assert!(c.key.is_none());
        assert_eq!(c.modifiers, vec![Modifier::Shift]);
    }

    #[test]
    fn parse_empty_is_error() {
        assert!(KeyCombo::parse("").is_err());
        assert!(KeyCombo::parse("+").is_err());
    }

    #[test]
    fn parse_two_keys_is_error() {
        assert!(KeyCombo::parse("a+b").is_err());
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test injector:: 2>&1 | tail -15`
Expected: FAIL — type mismatch (`c.key` is `String`, not `Option`); `parse("ctrl+shift")` currently errors.

- [ ] **Step 3: Change `KeyCombo` and `parse`**

In `src/injector/mod.rs`, change the struct and parse:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyCombo {
    pub modifiers: Vec<Modifier>,
    pub key: Option<String>,
}
```

```rust
impl KeyCombo {
    pub fn parse(s: &str) -> Result<Self> {
        let lower = s.to_lowercase();
        let mut modifiers = Vec::new();
        let mut key: Option<String> = None;

        for part in lower.split('+').map(str::trim) {
            if part.is_empty() {
                continue; // tolerate trailing/double '+'
            }
            match part {
                "ctrl" | "control" => modifiers.push(Modifier::Ctrl),
                "shift"            => modifiers.push(Modifier::Shift),
                "alt"              => modifiers.push(Modifier::Alt),
                "windows" | "win" | "super" => modifiers.push(Modifier::Windows),
                k => {
                    if key.is_some() {
                        bail!("multiple non-modifier keys in combo: {}", s);
                    }
                    key = Some(k.to_string());
                }
            }
        }

        if key.is_none() && modifiers.is_empty() {
            bail!("empty combo: {}", s);
        }
        Ok(Self { modifiers, key })
    }
}
```

- [ ] **Step 4: Update `press` in `windows.rs` to handle the Option key**

In `src/injector/windows.rs`, replace the `press` method body:

```rust
    fn press(&self, combo: &KeyCombo) -> Result<()> {
        let vk = match &combo.key {
            Some(k) => Some(*self.key_map.get(k)
                .with_context(|| format!("unknown key: {}", k))?),
            None => None,
        };

        let mut inputs: Vec<INPUT> = Vec::new();
        for m in &combo.modifiers {
            inputs.push(Self::make_input(Self::modifier_vk(m), 0));
        }
        if let Some(vk) = vk { inputs.push(Self::make_input(vk, 0)); }
        if let Some(vk) = vk { inputs.push(Self::make_input(vk, KEYEVENTF_KEYUP)); }
        for m in combo.modifiers.iter().rev() {
            inputs.push(Self::make_input(Self::modifier_vk(m), KEYEVENTF_KEYUP));
        }

        let sent = unsafe {
            SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32)
        };
        if sent == 0 {
            log::warn!("SendInput returned 0 for combo {:?}", combo);
        }
        Ok(())
    }
```

- [ ] **Step 5: Update `combo_valid` + hints in `monitor/mod.rs`**

In `src/monitor/mod.rs`, update the `combo_valid` helper to accept a `None` key:

```rust
fn combo_valid(s: &str, valid_keys: &HashSet<String>) -> bool {
    match KeyCombo::parse(s) {
        Ok(c) => match &c.key {
            Some(k) => valid_keys.contains(k),
            None => true, // modifier-only combo (e.g. "shift")
        },
        Err(_) => false,
    }
}
```

In `render_bindings`, extend the hints `ui.weak(...)` line to mention modifier-only + media keys — replace the existing hints text with:

```rust
        ui.weak("Combo = optional modifiers (ctrl / shift / alt / win) + one key, held while \
                 the G-key is held. Modifiers alone are allowed (e.g. shift, ctrl+shift). \
                 Keys: a-z, 0-9, f1-f24, enter, esc, space, tab, arrows, home/end, \
                 pageup/pagedown, insert/delete, and media: playpause, nexttrack, prevtrack, \
                 volup, voldown, mute (media keys tap). Empty = unmapped.");
```

- [ ] **Step 6: Run to verify pass**

Run: `cargo test 2>&1 | tail -5`
Expected: PASS — parser tests green; `press`/`combo_valid` compile with the Option key. (Dispatcher still calls `press` on KeyDown — unchanged behavior this task.)

- [ ] **Step 7: Commit**

```bash
git add src/injector/mod.rs src/injector/windows.rs src/monitor/mod.rs
git commit -m "feat: allow modifier-only combos (KeyCombo.key is now Option)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: Injector combo_down / combo_up

**Files:** Modify `src/injector/mod.rs` (trait), `src/injector/windows.rs` (impl), `src/dispatcher.rs` (test MockInjector).

**Interfaces:**
- Produces: `KeyInjector::combo_down(&self, &KeyCombo) -> Result<()>`, `combo_up(&self, &KeyCombo) -> Result<()>`.

- [ ] **Step 1: Add the trait methods**

In `src/injector/mod.rs`, add to the `KeyInjector` trait:

```rust
pub trait KeyInjector: Send + Sync {
    fn press(&self, combo: &KeyCombo) -> Result<()>;
    /// Press and hold a single key down (no release). For joystick hold-to-move.
    fn key_down(&self, key: &str) -> Result<()>;
    /// Release a single key previously held with `key_down`.
    fn key_up(&self, key: &str) -> Result<()>;
    /// Press modifiers + key down and hold them (for hold-means-hold G-keys).
    fn combo_down(&self, combo: &KeyCombo) -> Result<()>;
    /// Release a combo previously held with `combo_down`.
    fn combo_up(&self, combo: &KeyCombo) -> Result<()>;
}
```

- [ ] **Step 2: Implement on `WindowsInjector`**

In `src/injector/windows.rs`, add a batch-send helper to the private `impl WindowsInjector` block (next to `send`):

```rust
    fn send_batch(inputs: &[INPUT], what: &str) {
        if inputs.is_empty() { return; }
        let sent = unsafe {
            SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32)
        };
        if sent == 0 {
            log::warn!("SendInput returned 0 for {what}");
        }
    }
```

Add the two methods inside `impl KeyInjector for WindowsInjector` (after `key_up`):

```rust
    fn combo_down(&self, combo: &KeyCombo) -> Result<()> {
        let mut inputs: Vec<INPUT> = Vec::new();
        for m in &combo.modifiers {
            inputs.push(Self::make_input(Self::modifier_vk(m), 0));
        }
        if let Some(k) = &combo.key {
            let vk = *self.key_map.get(k).with_context(|| format!("unknown key: {}", k))?;
            inputs.push(Self::make_input(vk, 0));
        }
        Self::send_batch(&inputs, "combo_down");
        Ok(())
    }

    fn combo_up(&self, combo: &KeyCombo) -> Result<()> {
        let mut inputs: Vec<INPUT> = Vec::new();
        if let Some(k) = &combo.key {
            let vk = *self.key_map.get(k).with_context(|| format!("unknown key: {}", k))?;
            inputs.push(Self::make_input(vk, KEYEVENTF_KEYUP));
        }
        for m in combo.modifiers.iter().rev() {
            inputs.push(Self::make_input(Self::modifier_vk(m), KEYEVENTF_KEYUP));
        }
        Self::send_batch(&inputs, "combo_up");
        Ok(())
    }
```

- [ ] **Step 3: Update the test `MockInjector` to record combo_down/up**

In `src/dispatcher.rs` test module, the `MockInjector` implements `KeyInjector`; add the two methods and a recording. Add fields + a constructor and impl the methods. Add to the `MockInjector` struct definition two vecs, and add a constructor `new_combos()` returning the down/up handles, and implement `combo_down`/`combo_up` on the mock:

```rust
    // add to the struct:
    //   combo_downs: Arc<Mutex<Vec<KeyCombo>>>,
    //   combo_ups: Arc<Mutex<Vec<KeyCombo>>>,
    // and initialise them (Arc::new(Mutex::new(Vec::new()))) in every MockInjector constructor.

    impl MockInjector {
        fn new_combos() -> (Self, Arc<Mutex<Vec<KeyCombo>>>, Arc<Mutex<Vec<KeyCombo>>>) {
            let combos = Arc::new(Mutex::new(Vec::new()));
            let holds = Arc::new(Mutex::new(Vec::new()));
            let combo_downs = Arc::new(Mutex::new(Vec::new()));
            let combo_ups = Arc::new(Mutex::new(Vec::new()));
            (
                Self { combos, holds, combo_downs: combo_downs.clone(), combo_ups: combo_ups.clone() },
                combo_downs,
                combo_ups,
            )
        }
    }

    // in impl KeyInjector for MockInjector, add:
        fn combo_down(&self, combo: &KeyCombo) -> anyhow::Result<()> {
            self.combo_downs.lock().unwrap().push(combo.clone());
            Ok(())
        }
        fn combo_up(&self, combo: &KeyCombo) -> anyhow::Result<()> {
            self.combo_ups.lock().unwrap().push(combo.clone());
            Ok(())
        }
```

Update every existing `MockInjector { ... }` struct literal in the test module (in `new`, `new_with_holds`) to also initialise `combo_downs`/`combo_ups` with `Arc::new(Mutex::new(Vec::new()))`.

- [ ] **Step 4: Build + test**

Run: `cargo build 2>&1 | tail -2` → `Finished` (the new trait methods compile; MockInjector satisfies the trait).
Run: `cargo test 2>&1 | tail -3` → green (behavior unchanged; the dispatcher still uses `press` until Task 4).

- [ ] **Step 5: Commit**

```bash
git add src/injector/mod.rs src/injector/windows.rs src/dispatcher.rs
git commit -m "feat: add combo_down/combo_up to hold and release a whole combo

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: Dispatcher hold-means-hold

**Files:** Modify `src/dispatcher.rs`.

**Interfaces:**
- Consumes: `combo_down`/`combo_up` (Task 3), `tap_only_keys` (Task 1), `KeyCombo` with Option key (Task 2).

- [ ] **Step 1: Write the failing tests**

In `src/dispatcher.rs` test module, add (these use `new_combos()` from Task 3):

```rust
    #[test]
    fn gkey_holds_and_releases() {
        let (injector, downs, ups) = MockInjector::new_combos();
        let mut d = Dispatcher::new(make_config(&[("G1", "ctrl+c")]), Box::new(injector));
        d.handle(G13Event::KeyDown(G13Key::G1)).unwrap();
        assert_eq!(downs.lock().unwrap().len(), 1);
        assert_eq!(downs.lock().unwrap()[0].key.as_deref(), Some("c"));
        assert!(ups.lock().unwrap().is_empty());
        d.handle(G13Event::KeyUp(G13Key::G1)).unwrap();
        assert_eq!(ups.lock().unwrap()[0].key.as_deref(), Some("c"));
    }

    #[test]
    fn gkey_modifier_only_holds() {
        let (injector, downs, _ups) = MockInjector::new_combos();
        let mut d = Dispatcher::new(make_config(&[("G1", "shift")]), Box::new(injector));
        d.handle(G13Event::KeyDown(G13Key::G1)).unwrap();
        assert!(downs.lock().unwrap()[0].key.is_none());
        assert_eq!(downs.lock().unwrap()[0].modifiers, vec![Modifier::Shift]);
    }

    #[test]
    fn media_key_taps_not_held() {
        let (injector, downs, ups) = MockInjector::new_combos();
        let calls = injector.combos.clone(); // press() recording
        let mut d = Dispatcher::new(make_config(&[("G1", "playpause")]), Box::new(injector));
        d.handle(G13Event::KeyDown(G13Key::G1)).unwrap();
        assert!(downs.lock().unwrap().is_empty(), "media key should not be held");
        assert_eq!(calls.lock().unwrap().len(), 1, "media key should tap via press");
        d.handle(G13Event::KeyUp(G13Key::G1)).unwrap();
        assert!(ups.lock().unwrap().is_empty(), "no release for a tapped media key");
    }

    #[test]
    fn release_held_lifts_held_gkeys() {
        let (injector, _downs, ups) = MockInjector::new_combos();
        let mut d = Dispatcher::new(make_config(&[("G1", "w")]), Box::new(injector));
        d.handle(G13Event::KeyDown(G13Key::G1)).unwrap();
        d.release_held();
        assert_eq!(ups.lock().unwrap()[0].key.as_deref(), Some("w"));
    }
```

(Note: `media_key_taps_not_held` reads `injector.combos` before moving the injector into the dispatcher; the mock's `combos` field is `pub(self)` within the test module — accessible. If the field is private to the struct, use a constructor variant that also returns the combos handle, or clone it before the move as shown.)

- [ ] **Step 2: Run to verify failure**

Run: `cargo test dispatcher:: 2>&1 | tail -20`
Expected: FAIL — the dispatcher still `press`es on KeyDown and ignores KeyUp; `held_keys`/tap-only don't exist.

- [ ] **Step 3: Implement hold-means-hold**

In `src/dispatcher.rs`, update the imports, struct, `new`, `handle`, `handle_key`, add `handle_key_up`, and split the release methods. Replace lines 1-93 (imports through `release_held`) with:

```rust
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use crate::config::{JoystickMode, ProfileSet};
use crate::injector::{KeyCombo, KeyInjector};
use crate::injector::key_map::tap_only_keys;
use crate::joystick::{HoldAction, JoystickMapper};
use crate::protocol::{G13Event, G13Key, MKey};

pub struct Dispatcher {
    profiles: Arc<RwLock<ProfileSet>>,
    injector: Box<dyn KeyInjector>,
    joystick: JoystickMapper,
    held_keys: HashMap<G13Key, KeyCombo>,
    tap_only: HashSet<String>,
}

impl Dispatcher {
    pub fn new(profiles: Arc<RwLock<ProfileSet>>, injector: Box<dyn KeyInjector>) -> Self {
        Self {
            profiles,
            injector,
            joystick: JoystickMapper::new(),
            held_keys: HashMap::new(),
            tap_only: tap_only_keys(),
        }
    }

    pub fn handle(&mut self, event: G13Event) -> Result<()> {
        match event {
            G13Event::KeyDown(key) => self.handle_key_down(key),
            G13Event::KeyUp(key) => self.handle_key_up(key),
            G13Event::JoystickMove { x, y } => self.handle_joystick(x, y),
            G13Event::MKeyDown(m) => self.handle_mkey(m),
            G13Event::MKeyUp(_) => {}
        }
        Ok(())
    }

    fn handle_key_down(&mut self, key: G13Key) {
        let binding = {
            let set = self.profiles.read().unwrap();
            set.active_profile().get_binding(key).map(str::to_owned)
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
                Ok(()) => { self.held_keys.insert(key, combo); }
                Err(e) => log::warn!("injection failed: {e:#}"),
            }
        }
    }

    fn handle_key_up(&mut self, key: G13Key) {
        if let Some(combo) = self.held_keys.remove(&key) {
            if let Err(e) = self.injector.combo_up(&combo) {
                log::warn!("injection failed: {e:#}");
            }
        }
    }

    fn handle_joystick(&mut self, x: u8, y: u8) {
        let cfg = {
            let set = self.profiles.read().unwrap();
            set.active_profile().joystick()
                .filter(|j| j.mode == JoystickMode::Wasd)
                .cloned()
        };
        let actions = match &cfg {
            Some(jc) => self.joystick.update(x, y, jc),
            None => Vec::new(),
        };
        self.apply(actions);
    }

    /// Switch profile on M1/M2/M3. Release held joystick keys first (a new profile
    /// may rebind the stick); held G-keys stay down until their physical KeyUp.
    fn handle_mkey(&mut self, m: MKey) {
        if m == MKey::MR { return; }
        self.release_joystick();
        let mut set = self.profiles.write().unwrap();
        if set.set_active(m) {
            log::info!("profile -> {}", set.name(m).unwrap_or("?"));
        } else {
            log::warn!("no profile bound to {m:?}");
        }
    }

    fn apply(&self, actions: Vec<HoldAction>) {
        for action in actions {
            log::debug!("joystick {action:?}");
            let result = match &action {
                HoldAction::KeyDown(k) => self.injector.key_down(k),
                HoldAction::KeyUp(k) => self.injector.key_up(k),
            };
            if let Err(e) = result {
                log::warn!("joystick injection failed for {action:?}: {e:#}");
            }
        }
    }

    fn release_joystick(&mut self) {
        let actions = self.joystick.release_all();
        self.apply(actions);
    }

    /// Release everything held — the joystick and all held G-key combos. Call on
    /// Active->Dry-run, USB disconnect, and shutdown so nothing sticks.
    pub fn release_held(&mut self) {
        self.release_joystick();
        for (_key, combo) in self.held_keys.drain() {
            if let Err(e) = self.injector.combo_up(&combo) {
                log::warn!("injection failed on release: {e:#}");
            }
        }
    }
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test 2>&1 | tail -6`
Expected: PASS — the four new tests + all prior. Note: existing dispatcher tests that asserted `press` on a normal KeyDown (e.g. `key_down_triggers_injection`, `two_keys_dispatched_independently`) now need to assert on `combo_downs` instead of `combos`. Update those tests: switch them to `MockInjector::new_combos()` and assert the combo's `.key`/`.modifiers` on the `downs` handle. (E.g. `key_down_triggers_injection`: G1=ctrl+c → `downs.lock().unwrap()[0].key == Some("c")`, `.modifiers == vec![Modifier::Ctrl]`.)

- [ ] **Step 5: Commit**

```bash
git add src/dispatcher.rs
git commit -m "feat: G-keys hold-means-hold; media keys tap; release held on lift

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: Hardware smoke test + docs

**Files:** Create `milestones/finished/hold-means-hold-media-keys.md`; modify `CLAUDE.md` (a conventions note).

- [ ] **Step 1: Build release + full test**

Run: `cargo build --release 2>&1 | tail -2` → `Finished`.
Run: `cargo test 2>&1 | tail -3` → green.

- [ ] **Step 2: Hardware smoke test (manual — G13 on WinUSB)**

Run the GUI, use the Bindings tab (Active mode) to set (and Save) on the default profile:
`G1 = shift`, `G2 = w`, `G3 = playpause`, `G4 = ctrl+c`.

```bash
export PATH="$HOME/.cargo/bin:/c/Strawberry/c/bin:$PATH"
export RUST_LOG=debug
./target/release/g13-driver.exe
```
Confirm ALL of:
- Editor accepts `shift` (modifier-only) and `playpause` (media) as valid (`ok`).
- **Hold a modifier:** hold **G1** (Shift held) and tap a letter key on your keyboard → capital letter; release G1 → back to lowercase. (Or hold G1 and tap **G4**? G4=ctrl+c — chording across G-keys: hold G1=Shift + the G4 combo.)
- **Hold a key:** in Notepad, hold **G2** → one `w` (held state; games would see W held); release → nothing stuck. Confirm no `w` remains stuck after release.
- **Media taps:** press **G3** → play/pause toggles once per press (does not repeat while held).
- **Combo:** tap **G4** → one Ctrl+C (copy). Holding G4 does not leave Ctrl stuck after release.
- **No stuck keys:** hold **G2**, then flip **Active→Dry-run** → `w` releases (release_held). Hold G2, unplug the G13 → `w` releases.

- [ ] **Step 3: Milestone + note**

Create `milestones/finished/hold-means-hold-media-keys.md`:

```markdown
# Hold-means-hold G-keys + multimedia keys

- **Status:** finished
- **Date:** 2026-07-05

## Outcome
Hardware-verified. G-key bindings are now hold-means-hold (held while the G-key is held),
enabling chording, held modifiers, and held movement keys. Multimedia keys were added to the
key map and are the tap-only exception. Spec:
`docs/superpowers/specs/2026-07-05-hold-means-hold-and-media-keys-design.md`; plan:
`docs/superpowers/plans/2026-07-05-hold-means-hold-and-media-keys.md`.
- `KeyCombo.key` is `Option<String>` (modifier-only combos like `shift` / `ctrl+shift`).
- Injector gained `combo_down`/`combo_up`; the dispatcher tracks `held_keys` per G-key
  (KeyDown holds or taps a media key, KeyUp releases); `release_held` lifts held G-keys +
  joystick on Dry-run/disconnect/shutdown; profile switch releases only the joystick.
- Media keys: playpause, nexttrack/next, prevtrack/prev, mediastop, volup/volumeup,
  voldown/volumedown, mute.

## Follow-ups
- Ctrl+C / force-kill graceful release (console control handler) — still open.
- More system keys (brightness, launch keys) via the same tap-only mechanism.
```

In `CLAUDE.md`, under the Conventions section, add a bullet:

```markdown
- **G-key bindings are hold-means-hold** (a G-key holds its bound key/combo while held);
  multimedia keys are the tap-only exception. `KeyCombo.key` is `Option<String>` so a G-key
  can hold a modifier alone. See `milestones/finished/hold-means-hold-media-keys.md`.
```

- [ ] **Step 4: Commit**

```bash
git add milestones/finished/hold-means-hold-media-keys.md CLAUDE.md
git commit -m "docs: record hold-means-hold + media keys milestone (hardware-verified)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Self-Review

**Spec coverage:**
- Media keys + `tap_only_keys` → Task 1 ✓
- `KeyCombo.key: Option<String>` + modifier-only parse; press/editor ripple → Task 2 ✓
- `combo_down`/`combo_up` (trait + WindowsInjector) → Task 3 ✓
- Dispatcher hold-means-hold (held_keys, tap-only tap, KeyUp release, release_held vs profile-switch split) → Task 4 ✓
- Editor validation (Option key + media) + hints → Task 2 ✓
- Testing (parser, key_map, dispatcher via MockInjector; GUI/SendInput manual) → Tasks 1,2,4 ✓
- Hardware smoke + milestone → Task 5 ✓

**Deviations:** none. Note: the parser tolerates empty split segments (`ctrl+` → bare `ctrl`), so trailing `+` yields a modifier-only combo rather than an error — acceptable and lenient.

**Placeholder scan:** none — every step has concrete code/commands.

**Type consistency:** `KeyCombo{modifiers, key: Option<String>}`, `KeyInjector::{press,key_down,key_up,combo_down,combo_up}`, `tap_only_keys() -> HashSet<String>`, `Dispatcher.{held_keys, tap_only}`, `handle_key_down`/`handle_key_up`/`release_joystick`/`release_held`, `MockInjector::new_combos` — consistent across tasks.
