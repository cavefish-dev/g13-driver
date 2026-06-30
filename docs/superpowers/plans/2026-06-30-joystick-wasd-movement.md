# Joystick → WASD Movement Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Decode the G13 analog joystick (report bytes 1, 2) and translate stick direction into held keystrokes (WASD-style, config-driven, 8-way).

**Architecture:** Joystick is a new `G13Event::JoystickMove` emitted by `ReportParser`. A pure `JoystickMapper` converts X/Y → key-hold transitions using per-axis thresholding; it holds only the held-key *state* and reads deadzone/keys live from config each move (matching how G-key bindings already hot-reload). The `KeyInjector` trait gains true `key_down`/`key_up`. The dispatcher routes `JoystickMove` to the mapper and injects the transitions.

**Tech Stack:** Rust, GNU toolchain (`stable-x86_64-pc-windows-gnu`), `rusb`, `windows-sys` (`SendInput`), `toml`/`serde`, `log`. Build/test with `cargo` (PATH may need `export PATH="$HOME/.cargo/bin:/c/Strawberry/c/bin:$PATH"`).

## Global Constraints

- **Windows-only** (`src/main.rs:1-2` enforces `compile_error!` off-Windows). OS code stays behind `#[cfg(windows)]` in `src/injector/`. Do NOT leak Win32 types into `protocol`/`config`/`dispatcher`/`joystick`.
- **TDD:** every pure-logic change is test-first (RED → GREEN). `SendInput` code (`src/injector/windows.rs`) has no unit tests — verified by the manual smoke test in Task 6.
- **Error policy:** injection failures `log::warn!` and continue — never `panic!`/`unwrap()` in the runtime path.
- **Verified report layout (hardware-confirmed):** byte 1 = joystick X (`0x00` left, `0x7F`=127 center, `0xFF` right); byte 2 = joystick Y (`0x00` up, `0x7F` center, `0xFF` down); bytes 3–5 = G-keys; bytes 6–7 = M-keys/click (NOT used in this plan).
- **Commits:** one focused commit per task; imperative subject. End commit messages with the `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>` trailer (matches repo history).
- Run tests with `cargo test` (binary crate — `cargo test --lib` fails; use `cargo test` or `cargo test <module>::`).

---

## File Structure

| File | Change | Responsibility |
|------|--------|----------------|
| `src/injector/mod.rs` | Modify | Add `key_down`/`key_up` to `KeyInjector` trait |
| `src/injector/windows.rs` | Modify | Implement `key_down`/`key_up`; refactor `press` to reuse them |
| `src/protocol.rs` | Modify | Add `G13Event::JoystickMove`; parser tracks prev X/Y |
| `src/config.rs` | Modify | Parse `[joystick]` → `JoystickConfig`/`JoystickMode` |
| `src/joystick.rs` | Create | `JoystickMapper`, `HoldAction` — pure X/Y → hold transitions |
| `src/dispatcher.rs` | Modify | Route `JoystickMove` through mapper to injector; release-on-shutdown |
| `src/main.rs` | Modify | Declare `mod joystick`; build dispatcher mut; release held keys after loop; mouse-mode startup log |
| `config.toml` | Modify | Add example `[joystick]` section |

---

## Task 1: Injector key-hold methods

**Files:**
- Modify: `src/injector/mod.rs:21-23` (trait)
- Modify: `src/injector/windows.rs:43-70` (impl)
- Modify: `src/dispatcher.rs` (test `MockInjector` must implement new methods)

**Interfaces:**
- Produces: `KeyInjector::key_down(&self, key: &str) -> Result<()>` and `KeyInjector::key_up(&self, key: &str) -> Result<()>`. `press()` unchanged in signature.

- [ ] **Step 1: Add the two methods to the trait**

In `src/injector/mod.rs`, replace the trait (lines 21-23):

```rust
pub trait KeyInjector: Send + Sync {
    fn press(&self, combo: &KeyCombo) -> Result<()>;
    /// Press and hold a single key down (no release). For joystick hold-to-move.
    fn key_down(&self, key: &str) -> Result<()>;
    /// Release a single key previously held with `key_down`.
    fn key_up(&self, key: &str) -> Result<()>;
}
```

- [ ] **Step 2: Implement them on WindowsInjector and refactor `press`**

In `src/injector/windows.rs`, replace the entire `impl KeyInjector for WindowsInjector` block (lines 43-70) with:

```rust
impl WindowsInjector {
    fn send(&self, key: &str, flags: u32) -> Result<()> {
        let vk = *self.key_map.get(key)
            .with_context(|| format!("unknown key: {}", key))?;
        let input = Self::make_input(vk, flags);
        let sent = unsafe {
            SendInput(1, &input, std::mem::size_of::<INPUT>() as i32)
        };
        if sent == 0 {
            log::warn!("SendInput returned 0 for key {} (flags {})", key, flags);
        }
        Ok(())
    }
}

impl KeyInjector for WindowsInjector {
    fn press(&self, combo: &KeyCombo) -> Result<()> {
        let vk = *self.key_map.get(&combo.key)
            .with_context(|| format!("unknown key: {}", combo.key))?;

        let mut inputs: Vec<INPUT> = Vec::new();
        for m in &combo.modifiers {
            inputs.push(Self::make_input(Self::modifier_vk(m), 0));
        }
        inputs.push(Self::make_input(vk, 0));
        inputs.push(Self::make_input(vk, KEYEVENTF_KEYUP));
        for m in combo.modifiers.iter().rev() {
            inputs.push(Self::make_input(Self::modifier_vk(m), KEYEVENTF_KEYUP));
        }

        let sent = unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                std::mem::size_of::<INPUT>() as i32,
            )
        };
        if sent == 0 {
            log::warn!("SendInput returned 0 for combo {:?}", combo);
        }
        Ok(())
    }

    fn key_down(&self, key: &str) -> Result<()> {
        self.send(key, 0)
    }

    fn key_up(&self, key: &str) -> Result<()> {
        self.send(key, KEYEVENTF_KEYUP)
    }
}
```

- [ ] **Step 3: Make the test `MockInjector` implement the new methods**

In `src/dispatcher.rs`, the test module has a `MockInjector` (around line 39-53). It records combos for the existing tests; add a second log for hold actions. Replace the `MockInjector` struct + its `new` + its `impl KeyInjector` with:

```rust
    struct MockInjector {
        combos: Arc<Mutex<Vec<KeyCombo>>>,
        holds: Arc<Mutex<Vec<String>>>,
    }

    impl MockInjector {
        fn new() -> (Self, Arc<Mutex<Vec<KeyCombo>>>) {
            let combos = Arc::new(Mutex::new(Vec::new()));
            let holds = Arc::new(Mutex::new(Vec::new()));
            (Self { combos: combos.clone(), holds }, combos)
        }
    }

    impl KeyInjector for MockInjector {
        fn press(&self, combo: &KeyCombo) -> anyhow::Result<()> {
            self.combos.lock().unwrap().push(combo.clone());
            Ok(())
        }
        fn key_down(&self, key: &str) -> anyhow::Result<()> {
            self.holds.lock().unwrap().push(format!("down:{}", key));
            Ok(())
        }
        fn key_up(&self, key: &str) -> anyhow::Result<()> {
            self.holds.lock().unwrap().push(format!("up:{}", key));
            Ok(())
        }
    }
```

(The existing tests reference `calls` = the returned combos handle; they keep working unchanged. Task 5 adds a `MockInjector::new_with_holds()` to also capture the holds vec.)

- [ ] **Step 4: Build and run the full suite — confirm no regressions**

Run: `cargo test 2>&1 | tail -5`
Expected: `test result: ok. 29 passed` (no new tests yet; this task is infrastructure for hold injection, verified to compile and not regress).

- [ ] **Step 5: Commit**

```bash
git add src/injector/mod.rs src/injector/windows.rs src/dispatcher.rs
git commit -m "feat: add key_down/key_up to KeyInjector for hold-to-move

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: JoystickMove event in the protocol parser

**Files:**
- Modify: `src/protocol.rs` (`G13Event` enum lines 8-12, `ReportParser` struct lines 14-16, `new` 19-21, `parse` 23-38, tests)

**Interfaces:**
- Produces: `G13Event::JoystickMove { x: u8, y: u8 }`. `ReportParser::new()` initialises prev X/Y to center (127) so a centered idle report emits no move.

- [ ] **Step 1: Write the failing tests**

In `src/protocol.rs` test module, ADD these tests (place after `idle_report_emits_no_events`). Also CHANGE the existing `no_keys_no_events` to use `idle()` instead of `empty()` (an all-zero report now means "stick full up-left", which correctly emits a move):

```rust
    #[test]
    fn no_keys_no_events() {
        let mut p = ReportParser::new();
        assert!(p.parse(&idle()).is_empty());
    }

    #[test]
    fn joystick_move_emitted_on_x_change() {
        let mut p = ReportParser::new();
        let mut r = idle();
        r[1] = 0x00; // stick full left
        assert_eq!(p.parse(&r), vec![G13Event::JoystickMove { x: 0x00, y: 0x7F }]);
    }

    #[test]
    fn joystick_no_move_when_centered_and_unchanged() {
        let mut p = ReportParser::new();
        p.parse(&idle());                 // first centered report
        assert!(p.parse(&idle()).is_empty()); // unchanged -> no move
    }

    #[test]
    fn key_and_joystick_move_together() {
        let mut p = ReportParser::new();
        let mut r = idle();
        r[1] = 0xFF;            // stick full right
        r[3] = 0b0000_0001;     // G1 down
        let events = p.parse(&r);
        assert!(events.contains(&G13Event::JoystickMove { x: 0xFF, y: 0x7F }));
        assert!(events.contains(&G13Event::KeyDown(G13Key::G1)));
        assert_eq!(events.len(), 2);
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test protocol:: 2>&1 | tail -15`
Expected: FAIL — compile error `no variant named JoystickMove found for enum G13Event`.

- [ ] **Step 3: Implement the variant and parser change**

In `src/protocol.rs`, replace the `G13Event` enum (lines 8-12):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum G13Event {
    KeyDown(G13Key),
    KeyUp(G13Key),
    JoystickMove { x: u8, y: u8 },
}
```

Replace the `ReportParser` struct and `new` (lines 14-21):

```rust
pub struct ReportParser {
    prev_keys: u32,
    prev_x: u8,
    prev_y: u8,
}

impl ReportParser {
    pub fn new() -> Self {
        Self { prev_keys: 0, prev_x: 127, prev_y: 127 }
    }
```

In `parse`, add joystick handling at the TOP of the method body (immediately after `pub fn parse(&mut self, report: &[u8; 8]) -> Vec<G13Event> {`), before the existing `let current = ...` line. The existing key logic stays; just push joystick first into a pre-created `events` vec. Replace the body from `let current` through the `let mut events = Vec::new();` line so the vec is created first:

```rust
    pub fn parse(&mut self, report: &[u8; 8]) -> Vec<G13Event> {
        let mut events = Vec::new();

        // Joystick: byte 1 = X, byte 2 = Y (verified on hardware).
        let x = report[1];
        let y = report[2];
        if x != self.prev_x || y != self.prev_y {
            self.prev_x = x;
            self.prev_y = y;
            events.push(G13Event::JoystickMove { x, y });
        }

        // G-key bitmask is bytes 3,4,5 (byte 3 = G1-G8, byte 4 = G9-G16,
        // byte 5 = G17-G22). Bytes 1,2 are the joystick X/Y axes (centered at
        // 0x7F) and byte 5 bit7 is a constant flag — none are keys. Verified
        // against real hardware; see milestones/.../02-hardware-bringup.md.
        let current = (report[3] as u32)
            | ((report[4] as u32) << 8)
            | ((report[5] as u32) << 16);

        let pressed  = current & !self.prev_keys;
        let released = self.prev_keys & !current;
        self.prev_keys = current;

        for bit in 0..22u32 {
            if pressed  & (1 << bit) != 0 { events.push(G13Event::KeyDown(Self::bit_to_key(bit))); }
            if released & (1 << bit) != 0 { events.push(G13Event::KeyUp(Self::bit_to_key(bit))); }
        }
        events
    }
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test protocol:: 2>&1 | tail -5`
Expected: PASS — all protocol tests green (now 11 tests).

- [ ] **Step 5: Commit**

```bash
git add src/protocol.rs
git commit -m "feat: emit JoystickMove events from report bytes 1,2

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: JoystickConfig parsing

**Files:**
- Modify: `src/config.rs` (add raw struct, types, `from_raw` wiring, accessor, tests)

**Interfaces:**
- Produces:
  - `pub enum JoystickMode { Wasd, Mouse }` (derives `Debug, Clone, PartialEq, Eq`)
  - `pub struct JoystickConfig { pub mode: JoystickMode, pub deadzone: u8, pub up: Option<String>, pub down: Option<String>, pub left: Option<String>, pub right: Option<String> }` (derives `Debug, Clone`)
  - `Config::joystick(&self) -> Option<&JoystickConfig>` — `None` when no `[joystick]` section.

- [ ] **Step 1: Write the failing tests**

In `src/config.rs` test module, add:

```rust
    #[test]
    fn no_joystick_section_is_none() {
        let config = Config::from_raw(raw(&[("G1", "ctrl+c")])).unwrap();
        assert!(config.joystick().is_none());
    }

    #[test]
    fn parses_joystick_section() {
        let src = r#"
[keys]
G1 = "ctrl+c"

[joystick]
mode = "wasd"
deadzone = 30
up = "w"
down = "s"
left = "a"
right = "d"
"#;
        let raw: RawConfig = toml::from_str(src).unwrap();
        let config = Config::from_raw(raw).unwrap();
        let j = config.joystick().expect("joystick config present");
        assert_eq!(j.mode, JoystickMode::Wasd);
        assert_eq!(j.deadzone, 30);
        assert_eq!(j.up.as_deref(), Some("w"));
        assert_eq!(j.right.as_deref(), Some("d"));
    }

    #[test]
    fn joystick_mode_defaults_to_wasd() {
        let src = r#"
[joystick]
deadzone = 10
"#;
        let raw: RawConfig = toml::from_str(src).unwrap();
        let config = Config::from_raw(raw).unwrap();
        assert_eq!(config.joystick().unwrap().mode, JoystickMode::Wasd);
        assert_eq!(config.joystick().unwrap().deadzone, 10);
    }

    #[test]
    fn deadzone_default_is_30() {
        let src = "[joystick]\nup = \"w\"\n";
        let raw: RawConfig = toml::from_str(src).unwrap();
        let config = Config::from_raw(raw).unwrap();
        assert_eq!(config.joystick().unwrap().deadzone, 30);
    }

    #[test]
    fn deadzone_over_127_is_error() {
        let src = "[joystick]\ndeadzone = 200\n";
        let raw: RawConfig = toml::from_str(src).unwrap();
        assert!(Config::from_raw(raw).is_err());
    }

    #[test]
    fn unknown_joystick_mode_is_error() {
        let src = "[joystick]\nmode = \"flight\"\n";
        let raw: RawConfig = toml::from_str(src).unwrap();
        assert!(Config::from_raw(raw).is_err());
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test config:: 2>&1 | tail -15`
Expected: FAIL — `cannot find type JoystickMode` / `no method named joystick`.

- [ ] **Step 3: Implement the config types and parsing**

In `src/config.rs`, add the raw joystick struct and `joystick` field. Replace `RawConfig` (lines 7-11):

```rust
#[derive(Debug, Deserialize, Clone)]
pub struct RawConfig {
    #[serde(default)]
    pub keys: HashMap<String, String>,
    #[serde(default)]
    pub joystick: Option<RawJoystick>,
}

#[derive(Debug, Deserialize, Clone)]
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

fn default_mode() -> String { "wasd".to_string() }
fn default_deadzone() -> u16 { 30 }
```

Replace the `Config` struct (lines 13-16):

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JoystickMode {
    Wasd,
    Mouse,
}

#[derive(Debug, Clone)]
pub struct JoystickConfig {
    pub mode: JoystickMode,
    pub deadzone: u8,
    pub up: Option<String>,
    pub down: Option<String>,
    pub left: Option<String>,
    pub right: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Config {
    key_bindings: HashMap<G13Key, String>,
    joystick: Option<JoystickConfig>,
}
```

In `from_raw` (lines 27-35), build the joystick config before `Ok(Self {...})`:

```rust
    pub(crate) fn from_raw(raw: RawConfig) -> Result<Self> {
        let mut key_bindings = HashMap::new();
        for (name, binding) in raw.keys {
            let key = parse_g13_key(&name)
                .with_context(|| format!("unknown G13 key: {}", name))?;
            key_bindings.insert(key, binding);
        }

        let joystick = match raw.joystick {
            Some(rj) => Some(parse_joystick(rj)?),
            None => None,
        };

        Ok(Self { key_bindings, joystick })
    }

    pub fn joystick(&self) -> Option<&JoystickConfig> {
        self.joystick.as_ref()
    }
```

Add the `parse_joystick` free function next to `parse_g13_key` (after line 57):

```rust
fn parse_joystick(rj: RawJoystick) -> Result<JoystickConfig> {
    let mode = match rj.mode.to_lowercase().as_str() {
        "wasd" => JoystickMode::Wasd,
        "mouse" => JoystickMode::Mouse,
        other => anyhow::bail!("unknown joystick mode: {} (expected wasd or mouse)", other),
    };
    if rj.deadzone > 127 {
        anyhow::bail!("joystick deadzone {} out of range (0-127)", rj.deadzone);
    }
    Ok(JoystickConfig {
        mode,
        deadzone: rj.deadzone as u8,
        up: rj.up,
        down: rj.down,
        left: rj.left,
        right: rj.right,
    })
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test config:: 2>&1 | tail -5`
Expected: PASS — all config tests green.

- [ ] **Step 5: Commit**

```bash
git add src/config.rs
git commit -m "feat: parse [joystick] config section with validation

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: JoystickMapper (pure X/Y → hold transitions)

**Files:**
- Create: `src/joystick.rs`

**Interfaces:**
- Consumes: `crate::config::JoystickConfig` (Task 3).
- Produces:
  - `pub enum HoldAction { KeyDown(String), KeyUp(String) }` (derives `Debug, Clone, PartialEq, Eq`)
  - `pub struct JoystickMapper` with `pub fn new() -> Self`, `pub fn update(&mut self, x: u8, y: u8, cfg: &JoystickConfig) -> Vec<HoldAction>`, `pub fn release_all(&mut self) -> Vec<HoldAction>`.

- [ ] **Step 1: Write the failing tests**

Create `src/joystick.rs` with ONLY the test module first (implementation comes in Step 3):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{JoystickConfig, JoystickMode};

    fn wasd(deadzone: u8) -> JoystickConfig {
        JoystickConfig {
            mode: JoystickMode::Wasd,
            deadzone,
            up: Some("w".into()),
            down: Some("s".into()),
            left: Some("a".into()),
            right: Some("d".into()),
        }
    }

    #[test]
    fn centered_emits_nothing() {
        let mut m = JoystickMapper::new();
        assert!(m.update(127, 127, &wasd(30)).is_empty());
    }

    #[test]
    fn inside_deadzone_emits_nothing() {
        let mut m = JoystickMapper::new();
        // deadzone 30 -> fires only below 97 or above 157
        assert!(m.update(100, 150, &wasd(30)).is_empty());
    }

    #[test]
    fn full_left_presses_a() {
        let mut m = JoystickMapper::new();
        assert_eq!(m.update(0, 127, &wasd(30)), vec![HoldAction::KeyDown("a".into())]);
    }

    #[test]
    fn full_right_presses_d() {
        let mut m = JoystickMapper::new();
        assert_eq!(m.update(255, 127, &wasd(30)), vec![HoldAction::KeyDown("d".into())]);
    }

    #[test]
    fn full_up_presses_w() {
        let mut m = JoystickMapper::new();
        assert_eq!(m.update(127, 0, &wasd(30)), vec![HoldAction::KeyDown("w".into())]);
    }

    #[test]
    fn full_down_presses_s() {
        let mut m = JoystickMapper::new();
        assert_eq!(m.update(127, 255, &wasd(30)), vec![HoldAction::KeyDown("s".into())]);
    }

    #[test]
    fn return_to_center_releases() {
        let mut m = JoystickMapper::new();
        m.update(0, 127, &wasd(30));                    // hold a
        assert_eq!(m.update(127, 127, &wasd(30)), vec![HoldAction::KeyUp("a".into())]);
    }

    #[test]
    fn diagonal_holds_two_keys() {
        let mut m = JoystickMapper::new();
        let actions = m.update(0, 0, &wasd(30));        // up-left
        assert!(actions.contains(&HoldAction::KeyDown("a".into())));
        assert!(actions.contains(&HoldAction::KeyDown("w".into())));
        assert_eq!(actions.len(), 2);
    }

    #[test]
    fn cross_center_left_to_right_swaps_without_stuck_key() {
        let mut m = JoystickMapper::new();
        m.update(0, 127, &wasd(30));                    // hold a
        let actions = m.update(255, 127, &wasd(30));    // jump full right
        assert_eq!(actions, vec![
            HoldAction::KeyUp("a".into()),
            HoldAction::KeyDown("d".into()),
        ]);
    }

    #[test]
    fn holding_in_zone_is_idempotent() {
        let mut m = JoystickMapper::new();
        m.update(0, 127, &wasd(30));                    // hold a
        assert!(m.update(10, 127, &wasd(30)).is_empty()); // still left, no new event
    }

    #[test]
    fn release_all_lifts_held_keys() {
        let mut m = JoystickMapper::new();
        m.update(0, 0, &wasd(30));                      // hold a + w
        let mut released = m.release_all();
        released.sort_by(|x, y| format!("{:?}", x).cmp(&format!("{:?}", y)));
        assert_eq!(released, vec![
            HoldAction::KeyUp("a".into()),
            HoldAction::KeyUp("w".into()),
        ]);
        assert!(m.release_all().is_empty());            // second call: nothing
    }

    #[test]
    fn unmapped_direction_emits_nothing() {
        let mut cfg = wasd(30);
        cfg.up = None;
        let mut m = JoystickMapper::new();
        assert!(m.update(127, 0, &cfg).is_empty());     // up is unmapped
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test joystick:: 2>&1 | tail -15`
Expected: FAIL — `cannot find struct JoystickMapper` (module not declared yet AND type missing). Note: if `mod joystick;` is not yet in `main.rs`, the tests won't run at all — that is expected; Task 5/6 declares it. To run this task's tests now, temporarily add `mod joystick;` to `src/main.rs` (it is added permanently in Task 6, Step 1).

- [ ] **Step 3: Implement JoystickMapper above the test module**

Prepend to `src/joystick.rs` (before the `#[cfg(test)]` module):

```rust
use crate::config::JoystickConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HoldAction {
    KeyDown(String),
    KeyUp(String),
}

/// Converts analog joystick X/Y into key-hold transitions using independent
/// per-axis thresholding (8-way: a diagonal holds two keys). Holds only the
/// current held-key state; deadzone and key bindings are read from the config
/// passed to `update`, so config hot-reload takes effect live.
pub struct JoystickMapper {
    x_held: Option<String>,
    y_held: Option<String>,
}

const CENTER: i32 = 127;

impl JoystickMapper {
    pub fn new() -> Self {
        Self { x_held: None, y_held: None }
    }

    pub fn update(&mut self, x: u8, y: u8, cfg: &JoystickConfig) -> Vec<HoldAction> {
        let mut actions = Vec::new();
        let want_x = Self::target(x, cfg.deadzone, &cfg.left, &cfg.right);
        Self::diff(&mut actions, &mut self.x_held, want_x);
        let want_y = Self::target(y, cfg.deadzone, &cfg.up, &cfg.down);
        Self::diff(&mut actions, &mut self.y_held, want_y);
        actions
    }

    pub fn release_all(&mut self) -> Vec<HoldAction> {
        let mut actions = Vec::new();
        if let Some(k) = self.x_held.take() { actions.push(HoldAction::KeyUp(k)); }
        if let Some(k) = self.y_held.take() { actions.push(HoldAction::KeyUp(k)); }
        actions
    }

    /// Which key (if any) a single axis wants held, given its low/high targets.
    fn target(value: u8, deadzone: u8, low: &Option<String>, high: &Option<String>) -> Option<String> {
        let v = value as i32;
        let dz = deadzone as i32;
        if v < CENTER - dz {
            low.clone()
        } else if v > CENTER + dz {
            high.clone()
        } else {
            None
        }
    }

    /// Emit transitions to move one axis from its current held key to `want`.
    fn diff(actions: &mut Vec<HoldAction>, held: &mut Option<String>, want: Option<String>) {
        if *held == want {
            return;
        }
        if let Some(k) = held.take() {
            actions.push(HoldAction::KeyUp(k));
        }
        if let Some(k) = &want {
            actions.push(HoldAction::KeyDown(k.clone()));
        }
        *held = want;
    }
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test joystick:: 2>&1 | tail -5`
Expected: PASS — all JoystickMapper tests green.

- [ ] **Step 5: Commit**

```bash
git add src/joystick.rs src/main.rs
git commit -m "feat: add JoystickMapper - per-axis thresholding to key holds

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: Dispatch JoystickMove through the mapper

**Files:**
- Modify: `src/dispatcher.rs` (struct, `new`, `handle`, add `release_held`, tests)

**Interfaces:**
- Consumes: `JoystickMapper`/`HoldAction` (Task 4), `Config::joystick()`/`JoystickMode` (Task 3), `KeyInjector::key_down`/`key_up` (Task 1), `G13Event::JoystickMove` (Task 2).
- Produces: `Dispatcher::handle(&mut self, event)` (now `&mut self`), `Dispatcher::release_held(&mut self)`.

- [ ] **Step 1: Write the failing tests**

In `src/dispatcher.rs` test module, first add a helper to `MockInjector` to expose the holds vec (the `new` from Task 1 only returns the combos handle):

```rust
    impl MockInjector {
        fn new_with_holds() -> (Self, Arc<Mutex<Vec<String>>>) {
            let combos = Arc::new(Mutex::new(Vec::new()));
            let holds = Arc::new(Mutex::new(Vec::new()));
            (Self { combos, holds: holds.clone() }, holds)
        }
    }
```

Then add a config helper and tests:

```rust
    fn config_with_joystick() -> Arc<RwLock<Config>> {
        let src = r#"
[keys]
[joystick]
mode = "wasd"
deadzone = 30
up = "w"
down = "s"
left = "a"
right = "d"
"#;
        let raw: RawConfig = toml::from_str(src).unwrap();
        Arc::new(RwLock::new(Config::from_raw(raw).unwrap()))
    }

    #[test]
    fn joystick_move_left_holds_key() {
        let (injector, holds) = MockInjector::new_with_holds();
        let mut d = Dispatcher::new(config_with_joystick(), Box::new(injector));
        d.handle(G13Event::JoystickMove { x: 0, y: 127 }).unwrap();
        assert_eq!(*holds.lock().unwrap(), vec!["down:a".to_string()]);
    }

    #[test]
    fn joystick_return_to_center_releases_key() {
        let (injector, holds) = MockInjector::new_with_holds();
        let mut d = Dispatcher::new(config_with_joystick(), Box::new(injector));
        d.handle(G13Event::JoystickMove { x: 0, y: 127 }).unwrap();
        d.handle(G13Event::JoystickMove { x: 127, y: 127 }).unwrap();
        assert_eq!(*holds.lock().unwrap(), vec!["down:a".to_string(), "up:a".to_string()]);
    }

    #[test]
    fn joystick_ignored_when_no_config() {
        let (injector, holds) = MockInjector::new_with_holds();
        let config = make_config(&[("G1", "ctrl+c")]); // no [joystick]
        let mut d = Dispatcher::new(config, Box::new(injector));
        d.handle(G13Event::JoystickMove { x: 0, y: 127 }).unwrap();
        assert!(holds.lock().unwrap().is_empty());
    }

    #[test]
    fn release_held_lifts_keys() {
        let (injector, holds) = MockInjector::new_with_holds();
        let mut d = Dispatcher::new(config_with_joystick(), Box::new(injector));
        d.handle(G13Event::JoystickMove { x: 0, y: 0 }).unwrap(); // hold a + w
        holds.lock().unwrap().clear();
        d.release_held();
        let mut got = holds.lock().unwrap().clone();
        got.sort();
        assert_eq!(got, vec!["up:a".to_string(), "up:w".to_string()]);
    }
```

Update the existing tests that call `d.handle(...)` on a non-mut binding: change `let d = Dispatcher::new(...)` to `let mut d = Dispatcher::new(...)` in `key_down_triggers_injection`, `key_up_is_ignored`, `unmapped_key_does_nothing`, and `two_keys_dispatched_independently`. Also add `use crate::config::RawConfig;` to the test module's `use` block if not present.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test dispatcher:: 2>&1 | tail -15`
Expected: FAIL — `no method new_with_holds` / `JoystickMove` not handled / `handle` needs `&mut self`.

- [ ] **Step 3: Implement the dispatcher changes**

In `src/dispatcher.rs`, replace the `use` lines (1-5) and struct/impl down through `handle` (lines 7-29):

```rust
use anyhow::Result;
use std::sync::{Arc, RwLock};
use crate::config::{Config, JoystickMode};
use crate::injector::{KeyCombo, KeyInjector};
use crate::joystick::{HoldAction, JoystickMapper};
use crate::protocol::{G13Event, G13Key};

pub struct Dispatcher {
    config: Arc<RwLock<Config>>,
    injector: Box<dyn KeyInjector>,
    joystick: JoystickMapper,
}

impl Dispatcher {
    pub fn new(config: Arc<RwLock<Config>>, injector: Box<dyn KeyInjector>) -> Self {
        Self { config, injector, joystick: JoystickMapper::new() }
    }

    pub fn handle(&mut self, event: G13Event) -> Result<()> {
        match event {
            G13Event::KeyDown(key) => self.handle_key(key)?,
            G13Event::KeyUp(_) => {}
            G13Event::JoystickMove { x, y } => self.handle_joystick(x, y),
        }
        Ok(())
    }

    fn handle_key(&self, key: G13Key) -> Result<()> {
        let binding = {
            let cfg = self.config.read().unwrap();
            cfg.get_binding(key).map(str::to_owned)
        };
        match &binding {
            Some(b) => log::debug!("{key:?} -> {b}"),
            None => log::debug!("{key:?} -> (unmapped)"),
        }
        if let Some(binding) = binding {
            let combo = KeyCombo::parse(&binding)?;
            self.injector.press(&combo)?;
        }
        Ok(())
    }

    fn handle_joystick(&mut self, x: u8, y: u8) {
        // Read joystick config live so hot-reload takes effect. Clone so the
        // RwLock guard is released before we touch the injector.
        let cfg = {
            let guard = self.config.read().unwrap();
            guard.joystick()
                .filter(|j| j.mode == JoystickMode::Wasd)
                .cloned()
        };
        let actions = match &cfg {
            Some(jc) => self.joystick.update(x, y, jc),
            None => Vec::new(),
        };
        self.apply(actions);
    }

    fn apply(&self, actions: Vec<HoldAction>) {
        for action in actions {
            let result = match &action {
                HoldAction::KeyDown(k) => self.injector.key_down(k),
                HoldAction::KeyUp(k) => self.injector.key_up(k),
            };
            if let Err(e) = result {
                log::warn!("joystick injection failed for {action:?}: {e:#}");
            }
        }
    }

    /// Release every currently-held joystick key. Call on shutdown / USB error
    /// so a deflected stick does not leave keys stuck down.
    pub fn release_held(&mut self) {
        let actions = self.joystick.release_all();
        self.apply(actions);
    }
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test 2>&1 | tail -5`
Expected: PASS — full suite green (dispatcher + all prior tests).

- [ ] **Step 5: Commit**

```bash
git add src/dispatcher.rs
git commit -m "feat: route JoystickMove through mapper to held keystrokes

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 6: Wire into main, example config, hardware smoke test

**Files:**
- Modify: `src/main.rs` (declare `mod joystick`; `mut` dispatcher; release on shutdown; mouse-mode startup log)
- Modify: `config.toml` (example `[joystick]`)

**Interfaces:**
- Consumes: `Dispatcher::release_held` (Task 5), `Config::joystick`/`JoystickMode` (Task 3).

- [ ] **Step 1: Declare the joystick module**

In `src/main.rs`, add `mod joystick;` to the module list (after `mod injector;`, line 6). If Task 4 already added it temporarily, ensure it is present exactly once:

```rust
mod config;
mod dispatcher;
mod injector;
mod joystick;
mod protocol;
mod usb;
```

- [ ] **Step 2: Make the dispatcher mutable, release held keys after the loop, log mouse mode**

In `src/main.rs`, replace the dispatcher construction and dispatch loop (lines 36-47) with:

```rust
    let injector = Box::new(injector::windows::WindowsInjector::new());
    let mut dispatcher = dispatcher::Dispatcher::new(config.clone(), injector);

    if let Some(j) = config.read().unwrap().joystick() {
        if j.mode == config::JoystickMode::Mouse {
            log::warn!("joystick mouse mode is configured but not yet implemented; stick will be inert");
        }
    }

    log::info!("g13-driver running — press Ctrl+C to stop");

    for event in rx {
        if let Err(e) = dispatcher.handle(event) {
            log::warn!("dispatch error: {e:#}");
        }
    }

    // Channel closed (USB reader stopped / error): lift any held joystick keys.
    dispatcher.release_held();

    Ok(())
```

Note: `Dispatcher::new` already takes the `Arc<RwLock<Config>>`; this passes `config.clone()` so the local `config` remains usable for the mouse-mode check. If `config` was moved earlier, reorder so the clone happens before any move. (Current `main.rs` does not move `config` before this point — the watch thread got its own clone at line 23.)

- [ ] **Step 3: Build the release binary — confirm it compiles clean**

Run: `cargo build --release 2>&1 | tail -3`
Expected: `Finished \`release\` profile [optimized] target(s)` with no errors.

- [ ] **Step 4: Add the example joystick binding to config.toml**

Append to `config.toml`:

```toml

[joystick]
mode = "wasd"      # "wasd" (implemented) | "mouse" (parsed, not yet implemented)
deadzone = 30      # 0-127; distance from center (127) before a key fires
up = "w"
down = "s"
left = "a"
right = "d"
```

- [ ] **Step 5: Run the full test suite once more**

Run: `cargo test 2>&1 | tail -5`
Expected: PASS — full suite green.

- [ ] **Step 6: Hardware smoke test (manual — requires the G13 on WinUSB)**

Start the driver, then verify in Notepad:

```bash
export PATH="$HOME/.cargo/bin:/c/Strawberry/c/bin:$PATH"
export RUST_LOG=debug
./target/release/g13-driver.exe
```

Confirm ALL of:
- Open Notepad. Push stick **up** → `w` repeats; release to center → stops. Same for **down/left/right** (`s`/`a`/`d`).
- Push **up-left** diagonally → both `w` and `a` repeat together (8-way).
- Hold a direction, then **Ctrl+C the driver** → the held key does not stay stuck (release_held fired). Re-run and confirm no stuck key after exit.
- While running, edit `config.toml` `up = "w"` → `up = "i"`, save → log shows `config reloaded`; push up → `i` now types (live rebind).
- No `SendInput returned 0` warnings during normal use.

- [ ] **Step 7: Commit**

```bash
git add src/main.rs config.toml
git commit -m "feat: wire joystick WASD into main + example config; hardware-verified

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

- [ ] **Step 8: Update the milestone checklist**

In `milestones/open/v0.2-joystick-mkeys-service.md`, tick the joystick tasks (decode X/Y, map joystick → WASD) and add a note that this sub-project is complete; the M-key/profile and Windows Service sub-projects remain. Commit:

```bash
git add milestones/open/v0.2-joystick-mkeys-service.md
git commit -m "docs: mark v0.2 joystick->WASD sub-project complete

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Self-Review

**Spec coverage:**
- JoystickMove event + parser → Task 2 ✓
- JoystickMapper (update, release_all, per-axis 8-way, deadzone) → Task 4 ✓
- key_down/key_up + press refactor → Task 1 ✓
- `[joystick]` config (mode, deadzone, up/down/left/right; absent→disabled; deadzone/mode validation) → Task 3 ✓
- Dispatcher routing + release on shutdown/USB-error → Tasks 5 & 6 ✓
- Hot-reload while held (live config read; rebind self-corrects) → Task 5 `handle_joystick` reads config each move ✓
- `mode = "mouse"` parses but inert + log → Task 3 (parse) + Task 5 (filter Wasd → inert) + Task 6 (startup log) ✓
- Config schema example → Task 6 ✓
- Testing plan (mapper/parser/config coverage) → Tasks 2,3,4,5 ✓
- Stuck-keys-on-disconnect → Task 5 `release_held` + Task 6 call after loop ✓

**Deviations from spec (deliberate, simpler, match existing patterns):**
1. `JoystickMapper` does not cache config; `update` takes `&JoystickConfig` read live each move (mirrors live G-key binding reload). Removes rebuild-on-reload machinery; a rebind self-corrects on the next move. `release_all` retained for shutdown/USB-error.
2. Joystick key names are NOT validated at config load (only deadzone range + mode). Unknown keys surface as injection warnings — consistent with current G-key binding behavior and keeps `config.rs` free of any injector dependency.

**Placeholder scan:** none — every step has concrete code/commands.

**Type consistency:** `HoldAction::{KeyDown,KeyUp}(String)`, `JoystickMapper::{new,update,release_all}`, `JoystickConfig` fields, `JoystickMode::{Wasd,Mouse}`, `KeyInjector::{press,key_down,key_up}`, `G13Event::JoystickMove{x,y}` — names used identically across Tasks 1–6.
