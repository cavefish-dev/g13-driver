# RGB Backlight (v0.5) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Drive the G13 keypad backlight color and M-key mode LEDs from the active profile, configured via a global `[backlight]` config section and the GUI.

**Architecture:** A pure `led` module resolves (active M-slot + backlight config) into an `LedState { rgb, mkeys }`. A ~150 ms poller writes the resolved state into a shared `Arc<Mutex<LedState>>`; the USB reader thread (sole owner of the device handle) diffs that cell each loop tick and issues `SET_REPORT` control transfers, re-applying on reconnect. GUI edits and profile switches only mutate config — the poller reconciles everything.

**Tech Stack:** Rust, `rusb` (libusb), `toml`/`toml_edit`, `egui`/`eframe`.

## Global Constraints

- **GNU toolchain only.** Build/test with `stable-x86_64-pc-windows-gnu`; C compiler is MinGW gcc at `C:\Strawberry\c\bin\gcc.exe`. If `cargo`/`gcc` not found, prepend to PATH per CLAUDE.md. Do NOT switch to the MSVC target.
- **TDD** for all pure logic: failing test first (run it, confirm it fails), then minimal code, then green. USB control-transfer code is the documented exception — no unit tests, manual hardware verification only (same policy as the existing read path in `src/usb.rs`).
- **Error policy:** LED/USB failures `log::warn!` and continue. No `panic!`/`unwrap()` in the runtime path.
- **Platform isolation:** OS-specific USB code stays in `src/usb.rs`; don't leak `rusb`/Win32 types into `led`/`config`.
- **Colors** are `#RRGGBB` hex strings in config. **Brightness** is `f32` in `0.0..=1.0` (shown as 0–100% in the GUI). **M-key bitmask:** 1=M1, 2=M2, 4=M3 (MR → 0).
- One focused commit per task; imperative subject line.
- Reference: control transfers use `bmRequestType=0x21, bRequest=9`; color `wValue=0x0307`, M-LEDs `wValue=0x0305`; data is 5 bytes `[0x05, ...]`.

---

## File Structure

- **Create** `src/led/mod.rs` — pure logic: `Color`, `BacklightConfig`, `LedState`, `resolve()`, `color_packet()`, `mkey_packet()`, `spawn_poller()`.
- **Modify** `src/main.rs` — add `mod led;`.
- **Modify** `src/config.rs` — `RawBacklight`, a `backlight: BacklightConfig` field on `ProfileSet`, parse in `load()`, getters `backlight_config()`/`desired_led_state()`, setters, `persist_backlight()`.
- **Modify** `src/usb.rs` — `UsbReader::run` takes the shared `Arc<Mutex<LedState>>`; diff + control transfers; re-apply on reconnect.
- **Modify** `src/runtime.rs` — headless: create the cell, spawn the poller, pass the cell to `reader.run`.
- **Modify** `src/monitor/mod.rs` — GUI: same wiring in `start_consumer`; Settings-tab backlight block; Profiles-tab per-slot color pickers.
- **Modify** `config.toml` — commented `[backlight]` example.
- **Modify** `milestones/open/v0.5-rgb.md` → move to `milestones/ongoing/`; add smoke-test checklist.

---

## Task 1: `led::Color` with hex parse/format

**Files:**
- Create: `src/led/mod.rs`
- Modify: `src/main.rs` (add `mod led;` next to the other `mod` lines, ~line 13)

**Interfaces:**
- Produces: `pub struct Color(pub u8, pub u8, pub u8)`; `Color::from_hex(&str) -> Option<Color>`; `Color::to_hex(&self) -> String` (uppercase `#RRGGBB`).

- [ ] **Step 1: Write the failing test**

Add to `src/led/mod.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color(pub u8, pub u8, pub u8);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        assert_eq!(Color::from_hex("#FF3010"), Some(Color(0xFF, 0x30, 0x10)));
        assert_eq!(Color::from_hex("ff3010"), Some(Color(0xFF, 0x30, 0x10))); // no '#', lowercase
        assert_eq!(Color(0xFF, 0x30, 0x10).to_hex(), "#FF3010");
        assert_eq!(Color::from_hex("white"), None);   // malformed -> None
        assert_eq!(Color::from_hex("#FFF"), None);    // wrong length -> None
    }
}
```

Add `mod led;` to `src/main.rs`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test led::tests::hex_round_trip`
Expected: FAIL — `no function or associated item named from_hex`.

- [ ] **Step 3: Write minimal implementation**

In `src/led/mod.rs` (above the tests module):

```rust
impl Color {
    /// Parse `#RRGGBB` or `RRGGBB` (case-insensitive). `None` on any malformed input.
    pub fn from_hex(s: &str) -> Option<Color> {
        let h = s.strip_prefix('#').unwrap_or(s);
        if h.len() != 6 {
            return None;
        }
        let r = u8::from_str_radix(&h[0..2], 16).ok()?;
        let g = u8::from_str_radix(&h[2..4], 16).ok()?;
        let b = u8::from_str_radix(&h[4..6], 16).ok()?;
        Some(Color(r, g, b))
    }

    pub fn to_hex(&self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.0, self.1, self.2)
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test led::tests::hex_round_trip`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/led/mod.rs src/main.rs
git commit -m "feat(led): add Color type with hex parse/format"
```

---

## Task 2: `led` — `BacklightConfig`, `LedState`, `resolve`, packet builders

**Files:**
- Modify: `src/led/mod.rs`
- Test: `src/led/mod.rs` (tests module)

**Interfaces:**
- Consumes: `Color` (Task 1); `crate::protocol::MKey`.
- Produces:
  - `pub struct BacklightConfig { pub default_color: Color, pub brightness: f32, pub mkey_indicator: bool, pub slot_colors: [Option<Color>; 3] }` (derives `Clone, Copy`) with `Default` (white, `1.0`, `true`, `[None; 3]`).
  - `pub struct LedState { pub rgb: (u8, u8, u8), pub mkeys: u8 }` (derives `Clone, Copy, PartialEq, Eq, Debug`).
  - `pub fn resolve(active: MKey, cfg: &BacklightConfig) -> LedState`.
  - `pub fn color_packet(rgb: (u8, u8, u8)) -> [u8; 5]`; `pub fn mkey_packet(mask: u8) -> [u8; 5]`.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `src/led/mod.rs`:

```rust
use crate::protocol::MKey;

fn cfg() -> BacklightConfig {
    BacklightConfig {
        default_color: Color(200, 200, 200),
        brightness: 1.0,
        mkey_indicator: true,
        slot_colors: [Some(Color(255, 0, 0)), None, Some(Color(0, 0, 255))],
    }
}

#[test]
fn resolve_uses_slot_override() {
    let s = resolve(MKey::M1, &cfg());
    assert_eq!(s.rgb, (255, 0, 0));
    assert_eq!(s.mkeys, 1);
}

#[test]
fn resolve_falls_back_to_default() {
    let s = resolve(MKey::M2, &cfg()); // M2 has no override
    assert_eq!(s.rgb, (200, 200, 200));
    assert_eq!(s.mkeys, 2);
}

#[test]
fn resolve_scales_by_brightness() {
    let mut c = cfg();
    c.brightness = 0.5;
    let s = resolve(MKey::M1, &c); // (255,0,0) * 0.5
    assert_eq!(s.rgb, (128, 0, 0)); // 255*0.5 = 127.5 -> round -> 128
}

#[test]
fn resolve_brightness_zero_is_off() {
    let mut c = cfg();
    c.brightness = 0.0;
    assert_eq!(resolve(MKey::M3, &c).rgb, (0, 0, 0));
}

#[test]
fn resolve_indicator_off_clears_mkeys() {
    let mut c = cfg();
    c.mkey_indicator = false;
    assert_eq!(resolve(MKey::M3, &c).mkeys, 0);
}

#[test]
fn resolve_mr_has_no_indicator_and_default_color() {
    let s = resolve(MKey::MR, &cfg());
    assert_eq!(s.mkeys, 0);
    assert_eq!(s.rgb, (200, 200, 200));
}

#[test]
fn default_config_is_white_full_indicator() {
    let d = BacklightConfig::default();
    assert_eq!(d.default_color, Color(0xFF, 0xFF, 0xFF));
    assert_eq!(d.brightness, 1.0);
    assert!(d.mkey_indicator);
    assert_eq!(d.slot_colors, [None, None, None]);
}

#[test]
fn packets_have_expected_layout() {
    assert_eq!(color_packet((0x11, 0x22, 0x33)), [0x05, 0x11, 0x22, 0x33, 0x00]);
    assert_eq!(mkey_packet(0b0000_0100), [0x05, 0x04, 0x00, 0x00, 0x00]);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test led::`
Expected: FAIL — `cannot find type BacklightConfig` / `function resolve`.

- [ ] **Step 3: Write minimal implementation**

Add to `src/led/mod.rs` (above the tests module):

```rust
use crate::protocol::MKey;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BacklightConfig {
    pub default_color: Color,
    pub brightness: f32,
    pub mkey_indicator: bool,
    pub slot_colors: [Option<Color>; 3],
}

impl Default for BacklightConfig {
    fn default() -> Self {
        Self {
            default_color: Color(0xFF, 0xFF, 0xFF),
            brightness: 1.0,
            mkey_indicator: true,
            slot_colors: [None, None, None],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LedState {
    pub rgb: (u8, u8, u8),
    pub mkeys: u8,
}

/// Map an active M-slot to `slot_colors` index (M1/M2/M3); MR has no slot.
fn slot_index(m: MKey) -> Option<usize> {
    match m {
        MKey::M1 => Some(0),
        MKey::M2 => Some(1),
        MKey::M3 => Some(2),
        MKey::MR => None,
    }
}

/// Resolve (active slot + config) into the hardware LED state.
pub fn resolve(active: MKey, cfg: &BacklightConfig) -> LedState {
    let base = slot_index(active)
        .and_then(|i| cfg.slot_colors[i])
        .unwrap_or(cfg.default_color);
    let scale = cfg.brightness.clamp(0.0, 1.0);
    let scaled = |c: u8| (c as f32 * scale).round() as u8;
    let rgb = (scaled(base.0), scaled(base.1), scaled(base.2));
    let mkeys = if cfg.mkey_indicator {
        match active {
            MKey::M1 => 1,
            MKey::M2 => 2,
            MKey::M3 => 4,
            MKey::MR => 0,
        }
    } else {
        0
    };
    LedState { rgb, mkeys }
}

/// 5-byte SET_REPORT payload for the keypad backlight color (wValue 0x0307).
pub fn color_packet(rgb: (u8, u8, u8)) -> [u8; 5] {
    [0x05, rgb.0, rgb.1, rgb.2, 0x00]
}

/// 5-byte SET_REPORT payload for the M-key indicator LEDs (wValue 0x0305).
pub fn mkey_packet(mask: u8) -> [u8; 5] {
    [0x05, mask, 0x00, 0x00, 0x00]
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test led::`
Expected: PASS (all Task 1 + Task 2 tests).

- [ ] **Step 5: Commit**

```bash
git add src/led/mod.rs
git commit -m "feat(led): resolve backlight config to LedState + USB packets"
```

---

## Task 3: Parse `[backlight]` into `ProfileSet`

**Files:**
- Modify: `src/config.rs` (add `RawBacklight`; `backlight` field on `ProfileSet`; parse in both `load()` branches; add `backlight_config()` + `desired_led_state()`)
- Test: `src/config.rs` (tests module)

**Interfaces:**
- Consumes: `crate::led::{BacklightConfig, Color, LedState, resolve}`.
- Produces on `ProfileSet`: `pub fn backlight_config(&self) -> BacklightConfig`; `pub fn desired_led_state(&self) -> LedState`. New private field `backlight: BacklightConfig`.

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `src/config.rs` (near the other config tests; reuse the existing `tmp`/temp-dir helper pattern used by sibling tests):

```rust
#[test]
fn parses_backlight_section() {
    let d = std::env::temp_dir().join("g13-cfg-backlight");
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("profiles")).unwrap();
    std::fs::write(d.join("profiles/basic.toml"), "[keys]\nG1 = \"a\"\n").unwrap();
    std::fs::write(d.join("config.toml"),
        "profiles_dir = \"profiles\"\nm1 = \"basic.toml\"\n\
         [backlight]\ndefault_color = \"#102030\"\nbrightness = 0.5\n\
         mkey_indicator = false\nm1_color = \"#FF0000\"\n").unwrap();

    let set = ProfileSet::load(&d.join("config.toml")).unwrap();
    let b = set.backlight_config();
    assert_eq!(b.default_color, crate::led::Color(0x10, 0x20, 0x30));
    assert_eq!(b.brightness, 0.5);
    assert!(!b.mkey_indicator);
    assert_eq!(b.slot_colors[0], Some(crate::led::Color(0xFF, 0x00, 0x00)));
    assert_eq!(b.slot_colors[1], None);
}

#[test]
fn missing_backlight_section_uses_defaults() {
    let d = std::env::temp_dir().join("g13-cfg-nobacklight");
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("profiles")).unwrap();
    std::fs::write(d.join("profiles/basic.toml"), "[keys]\nG1 = \"a\"\n").unwrap();
    std::fs::write(d.join("config.toml"),
        "profiles_dir = \"profiles\"\nm1 = \"basic.toml\"\n").unwrap();

    let set = ProfileSet::load(&d.join("config.toml")).unwrap();
    assert_eq!(set.backlight_config(), crate::led::BacklightConfig::default());
    // active is M1 by default, default color white, indicator on -> mkeys = 1
    assert_eq!(set.desired_led_state(),
        crate::led::LedState { rgb: (255, 255, 255), mkeys: 1 });
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test config::tests::parses_backlight_section config::tests::missing_backlight_section_uses_defaults`
Expected: FAIL — `no method named backlight_config`.

- [ ] **Step 3: Write minimal implementation**

In `src/config.rs`:

Add the raw struct (near `RawApp`, ~line 287):

```rust
#[derive(Debug, Deserialize)]
struct RawBacklight {
    #[serde(default)]
    default_color: Option<String>,
    #[serde(default)]
    brightness: Option<f32>,
    #[serde(default)]
    mkey_indicator: Option<bool>,
    #[serde(default)]
    m1_color: Option<String>,
    #[serde(default)]
    m2_color: Option<String>,
    #[serde(default)]
    m3_color: Option<String>,
}
```

Add to `RawManifest` (after the `joystick` field, ~line 304):

```rust
    #[serde(default)]
    backlight: Option<RawBacklight>,
```

Add a `backlight: BacklightConfig` field to `struct ProfileSet` (after `joystick_deadzone`, ~line 322):

```rust
    backlight: crate::led::BacklightConfig,
```

In `load()`, after `let joystick_deadzone = ...;` (~line 340), build the config:

```rust
        let backlight = raw.backlight.map(|b| {
            use crate::led::{BacklightConfig, Color};
            let d = BacklightConfig::default();
            let parse = |s: Option<String>| s.and_then(|v| Color::from_hex(&v));
            BacklightConfig {
                default_color: parse(b.default_color).unwrap_or(d.default_color),
                brightness: b.brightness.unwrap_or(d.brightness).clamp(0.0, 1.0),
                mkey_indicator: b.mkey_indicator.unwrap_or(d.mkey_indicator),
                slot_colors: [parse(b.m1_color), parse(b.m2_color), parse(b.m3_color)],
            }
        }).unwrap_or_default();
```

Set `backlight` in BOTH `Self { ... }` constructors (manifest branch ~line 361 and legacy branch ~line 375), adding the field `backlight,`.

Add accessor methods in `impl ProfileSet` (near `joystick_deadzone`, ~line 392):

```rust
    pub fn backlight_config(&self) -> crate::led::BacklightConfig { self.backlight }

    /// The LED state the hardware should show for the current active slot + config.
    pub fn desired_led_state(&self) -> crate::led::LedState {
        crate::led::resolve(self.active, &self.backlight)
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test config::`
Expected: PASS (new tests + existing config tests still green).

- [ ] **Step 5: Commit**

```bash
git add src/config.rs
git commit -m "feat(config): parse [backlight] section into ProfileSet"
```

---

## Task 4: Backlight setters + `persist_backlight`

**Files:**
- Modify: `src/config.rs` (setters + `persist_backlight`)
- Test: `src/config.rs` (tests module)

**Interfaces:**
- Produces on `ProfileSet`:
  - `pub fn set_backlight_default_color(&mut self, c: Color)`
  - `pub fn set_backlight_brightness(&mut self, b: f32)` (clamps 0.0..=1.0)
  - `pub fn set_backlight_mkey_indicator(&mut self, on: bool)`
  - `pub fn set_backlight_slot_color(&mut self, slot: usize, c: Option<Color>)` (slot 0..=2)
  - `pub fn persist_backlight(&self) -> Result<()>` (format-preserving write of the whole `[backlight]` table; removes `mN_color` keys for `None` slots)

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `src/config.rs`:

```rust
#[test]
fn persist_backlight_round_trips() {
    use crate::led::Color;
    let d = std::env::temp_dir().join("g13-cfg-persistbl");
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("profiles")).unwrap();
    std::fs::write(d.join("profiles/basic.toml"), "[keys]\nG1 = \"a\"\n").unwrap();
    std::fs::write(d.join("config.toml"),
        "profiles_dir = \"profiles\"\nm1 = \"basic.toml\"\n").unwrap();

    let mut set = ProfileSet::load(&d.join("config.toml")).unwrap();
    set.set_backlight_default_color(Color(0x11, 0x22, 0x33));
    set.set_backlight_brightness(0.25);
    set.set_backlight_mkey_indicator(false);
    set.set_backlight_slot_color(0, Some(Color(0xAA, 0xBB, 0xCC)));
    set.set_backlight_slot_color(1, None);
    set.persist_backlight().unwrap();

    // Reload from disk and confirm the values survived.
    let reloaded = ProfileSet::load(&d.join("config.toml")).unwrap();
    let b = reloaded.backlight_config();
    assert_eq!(b.default_color, Color(0x11, 0x22, 0x33));
    assert_eq!(b.brightness, 0.25);
    assert!(!b.mkey_indicator);
    assert_eq!(b.slot_colors[0], Some(Color(0xAA, 0xBB, 0xCC)));
    assert_eq!(b.slot_colors[1], None);

    // Original manifest keys are preserved.
    let text = std::fs::read_to_string(d.join("config.toml")).unwrap();
    assert!(text.contains("m1 = \"basic.toml\""));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test config::tests::persist_backlight_round_trips`
Expected: FAIL — `no method named set_backlight_default_color`.

- [ ] **Step 3: Write minimal implementation**

Add setters in `impl ProfileSet` (near the other setters, after `set_joystick_deadzone`):

```rust
    pub fn set_backlight_default_color(&mut self, c: crate::led::Color) {
        self.backlight.default_color = c;
    }
    pub fn set_backlight_brightness(&mut self, b: f32) {
        self.backlight.brightness = b.clamp(0.0, 1.0);
    }
    pub fn set_backlight_mkey_indicator(&mut self, on: bool) {
        self.backlight.mkey_indicator = on;
    }
    pub fn set_backlight_slot_color(&mut self, slot: usize, c: Option<crate::led::Color>) {
        if slot < 3 {
            self.backlight.slot_colors[slot] = c;
        }
    }
```

Add the persist method (near `persist_joystick_deadzone`, ~line 438):

```rust
    /// Write the whole `[backlight]` table into the manifest, preserving every other
    /// key and comment (format-preserving via toml_edit). Best-effort; callers log on error.
    pub fn persist_backlight(&self) -> Result<()> {
        use toml_edit::{DocumentMut, Item, Table, value as toml_value};
        let text = std::fs::read_to_string(&self.config_path)
            .with_context(|| format!("failed to read {}", self.config_path.display()))?;
        let mut doc = text.parse::<DocumentMut>()
            .with_context(|| format!("failed to parse {}", self.config_path.display()))?;
        if !doc.as_table().contains_key("backlight") {
            doc.as_table_mut().insert("backlight", Item::Table(Table::new()));
        }
        let b = &self.backlight;
        doc["backlight"]["default_color"] = toml_value(b.default_color.to_hex());
        doc["backlight"]["brightness"] = toml_value(b.brightness as f64);
        doc["backlight"]["mkey_indicator"] = toml_value(b.mkey_indicator);
        for (i, key) in ["m1_color", "m2_color", "m3_color"].iter().enumerate() {
            match b.slot_colors[i] {
                Some(c) => { doc["backlight"][*key] = toml_value(c.to_hex()); }
                None => {
                    if let Some(t) = doc["backlight"].as_table_mut() {
                        t.remove(*key);
                    }
                }
            }
        }
        std::fs::write(&self.config_path, doc.to_string())
            .with_context(|| format!("failed to write {}", self.config_path.display()))?;
        Ok(())
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test config::tests::persist_backlight_round_trips`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/config.rs
git commit -m "feat(config): backlight setters + format-preserving persist"
```

---

## Task 5: USB write path — apply `LedState` from a shared cell

**Files:**
- Modify: `src/usb.rs` (`UsbReader::run` signature + apply loop)
- Modify: `src/runtime.rs` (`run_headless` supervisor passes a cell)
- Modify: `src/monitor/mod.rs` (`start_consumer` supervisor passes a cell)

**Interfaces:**
- Consumes: `crate::led::{LedState, color_packet, mkey_packet}`.
- Produces: `UsbReader::run(self, tx: Sender<G13Event>, desired: Arc<Mutex<LedState>>) -> Result<()>`.

**Note:** This task is USB code — **no unit test** (project policy). It must keep the build green: both call sites pass a cell, initialized to `desired_led_state()`. The poller that keeps the cell updated arrives in Task 6; after this task the LEDs show the startup state only.

- [ ] **Step 1: Change `UsbReader::run` to apply LED state**

In `src/usb.rs`, add imports at the top:

```rust
use std::sync::{Arc, Mutex};
use crate::led::{LedState, color_packet, mkey_packet};
```

Add a control-transfer constant near the others (~line 10):

```rust
// SET_REPORT: host->device, class, interface recipient.
const LED_REQUEST_TYPE: u8 = 0x21;
const LED_REQUEST: u8 = 0x09;
const LED_COLOR_VALUE: u16 = 0x0307;
const LED_MKEY_VALUE: u16 = 0x0305;
const LED_TIMEOUT: Duration = Duration::from_millis(100);
```

Replace the `run` method body:

```rust
    pub fn run(mut self, tx: Sender<G13Event>, desired: Arc<Mutex<LedState>>) -> Result<()> {
        let mut parser = ReportParser::new();
        let mut buf = [0u8; 8];
        // None so the first tick always applies the current desired state (also
        // re-applies after a reconnect, since `run` is called fresh each time).
        let mut last_applied: Option<LedState> = None;
        loop {
            self.apply_leds(&desired, &mut last_applied);
            match self.handle.read_interrupt(ENDPOINT_IN, &mut buf, READ_TIMEOUT) {
                Ok(8) => {
                    log::trace!("raw report: {buf:02X?}");
                    for event in parser.parse(&buf) {
                        if tx.send(event).is_err() {
                            return Ok(());
                        }
                    }
                }
                Ok(n) => log::warn!("unexpected report size: {n} bytes"),
                Err(rusb::Error::Timeout) => continue,
                Err(e) => {
                    return Err(anyhow::Error::from(e).context("USB read error"));
                }
            }
        }
    }

    /// Apply the desired LED state to the device when it changed. Warn-and-continue
    /// on transfer errors (a missed LED update is recoverable).
    fn apply_leds(&self, desired: &Arc<Mutex<LedState>>, last: &mut Option<LedState>) {
        let want = *desired.lock().unwrap();
        if *last == Some(want) {
            return;
        }
        let color = color_packet(want.rgb);
        if let Err(e) = self.handle.write_control(
            LED_REQUEST_TYPE, LED_REQUEST, LED_COLOR_VALUE, 0, &color, LED_TIMEOUT) {
            log::warn!("backlight color write failed: {e}");
            return; // leave `last` unchanged so we retry next tick
        }
        let mkeys = mkey_packet(want.mkeys);
        if let Err(e) = self.handle.write_control(
            LED_REQUEST_TYPE, LED_REQUEST, LED_MKEY_VALUE, 0, &mkeys, LED_TIMEOUT) {
            log::warn!("mkey LED write failed: {e}");
            return;
        }
        *last = Some(want);
    }
```

- [ ] **Step 2: Update the headless call site**

In `src/runtime.rs`, add `Mutex` to the imports (`use std::sync::{Arc, Mutex, RwLock};`). In `run_headless`, before the supervisor thread:

```rust
    let desired = Arc::new(Mutex::new(config.read().unwrap().desired_led_state()));
```

Change the supervisor closure to capture and pass it:

```rust
    let desired_sup = desired.clone();
    thread::spawn(move || loop {
        match usb::UsbReader::open() {
            Ok(reader) => {
                log::info!("G13 connected");
                let _ = reader.run(tx.clone(), desired_sup.clone());
                log::warn!("G13 disconnected — retrying");
            }
            Err(e) => log::warn!("G13 open failed: {e:#}"),
        }
        thread::sleep(Duration::from_secs(2));
    });
```

- [ ] **Step 3: Update the GUI call site**

In `src/monitor/mod.rs`, in `start_consumer` (~line 343), create the cell and pass it:

```rust
        let desired = std::sync::Arc::new(std::sync::Mutex::new(
            self.profiles.read().unwrap().desired_led_state()));
```

In the supervisor closure, capture `let desired_sup = desired.clone();` and change the run call:

```rust
                        let _ = reader.run(tx.clone(), desired_sup.clone()); // blocks until disconnect
```

- [ ] **Step 4: Build to verify it compiles**

Run: `cargo build`
Expected: builds clean (no unit test for the transfer).

- [ ] **Step 5: Commit**

```bash
git add src/usb.rs src/runtime.rs src/monitor/mod.rs
git commit -m "feat(usb): apply backlight + M-key LED state via control transfers"
```

---

## Task 6: LED poller — keep the shared cell in sync

**Files:**
- Modify: `src/led/mod.rs` (`spawn_poller`)
- Modify: `src/runtime.rs` (`run_headless` spawns the poller)
- Modify: `src/monitor/mod.rs` (`start_consumer` spawns the poller)

**Interfaces:**
- Consumes: `Arc<RwLock<ProfileSet>>`, `Arc<Mutex<LedState>>`.
- Produces: `pub fn spawn_poller(config: Arc<RwLock<crate::config::ProfileSet>>, desired: Arc<Mutex<LedState>>)`.

**Note:** The pure seam (`desired_led_state`) is already tested (Task 3). The poller thread is thin glue — no unit test; correctness is covered by the manual smoke test in Task 9.

- [ ] **Step 1: Add the poller**

In `src/led/mod.rs`, add at the top:

```rust
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::Duration;
use crate::config::ProfileSet;
```

Add the function (outside the tests module):

```rust
/// Poll the active profile + backlight config every ~150 ms and publish the
/// resolved LedState into the shared cell the USB reader consumes. Every change
/// source (device M-key, GUI edit, hot-reload) reconciles through this one loop.
pub fn spawn_poller(config: Arc<RwLock<ProfileSet>>, desired: Arc<Mutex<LedState>>) {
    thread::spawn(move || loop {
        let state = config.read().unwrap().desired_led_state();
        *desired.lock().unwrap() = state;
        thread::sleep(Duration::from_millis(150));
    });
}
```

- [ ] **Step 2: Spawn from headless**

In `src/runtime.rs` `run_headless`, right after creating `desired`:

```rust
    crate::led::spawn_poller(config.clone(), desired.clone());
```

- [ ] **Step 3: Spawn from the GUI**

In `src/monitor/mod.rs` `start_consumer`, right after creating `desired`:

```rust
        crate::led::spawn_poller(self.profiles.clone(), desired.clone());
```

- [ ] **Step 4: Build + run existing tests**

Run: `cargo test` then `cargo build`
Expected: all tests pass; build clean.

- [ ] **Step 5: Commit**

```bash
git add src/led/mod.rs src/runtime.rs src/monitor/mod.rs
git commit -m "feat(led): poll active profile into shared LedState cell"
```

---

## Task 7: GUI — Settings tab backlight block

**Files:**
- Modify: `src/monitor/mod.rs` (`render_settings`, ~line 1263)

**Interfaces:**
- Consumes: `ProfileSet::{backlight_config, set_backlight_default_color, set_backlight_brightness, set_backlight_mkey_indicator, persist_backlight}`; `crate::led::Color`.

**Note:** egui UI — verified manually (build + click), no unit test.

- [ ] **Step 1: Add the backlight block**

In `render_settings`, before the final `ui.weak(...)` about the joystick (after the deadzone slider block, ~line 1327), insert:

```rust
        ui.add_space(8.0);
        ui.separator();
        ui.label("Backlight");
        let cfg = self.profiles.read().unwrap().backlight_config();

        // Default color (used by any profile without its own color).
        let mut changed = false;
        let mut rgb = [cfg.default_color.0, cfg.default_color.1, cfg.default_color.2];
        ui.horizontal(|ui| {
            ui.label("Default color");
            if egui::color_picker::color_edit_button_srgb(ui, &mut rgb).changed() {
                changed = true;
            }
        });
        if changed {
            self.profiles.write().unwrap()
                .set_backlight_default_color(crate::led::Color(rgb[0], rgb[1], rgb[2]));
        }

        // Brightness 0-100%.
        let mut pct = (cfg.brightness * 100.0).round() as u32;
        if ui.add(egui::Slider::new(&mut pct, 0..=100).text("Brightness %")).changed() {
            self.profiles.write().unwrap().set_backlight_brightness(pct as f32 / 100.0);
            changed = true;
        }

        // M-key indicator toggle.
        let mut ind = cfg.mkey_indicator;
        if ui.checkbox(&mut ind, "Light active profile's M-key").changed() {
            self.profiles.write().unwrap().set_backlight_mkey_indicator(ind);
            changed = true;
        }

        if changed {
            if let Err(e) = self.profiles.read().unwrap().persist_backlight() {
                log::warn!("persist backlight failed: {e:#}");
            }
        }
        ui.weak("Applies to the whole keypad; brightness 0 turns the backlight off.");
```

**Borrow note:** each `read()`/`write()` guard is scoped to its statement (temporaries dropped at the `;`), so there is no read-while-write deadlock. Do not hold a guard across a `write()` call.

- [ ] **Step 2: Build**

Run: `cargo build`
Expected: builds clean.

- [ ] **Step 3: Manual check**

Run: `cargo run` → Settings tab. Confirm the color picker, brightness slider, and checkbox appear and that `config.toml` gains a `[backlight]` table after editing them.

- [ ] **Step 4: Commit**

```bash
git add src/monitor/mod.rs
git commit -m "feat(gui): backlight controls on the Settings tab"
```

---

## Task 8: GUI — Profiles tab per-slot color pickers

**Files:**
- Modify: `src/monitor/mod.rs` (`render_profiles`, the M-slot loop ~line 740-753)

**Interfaces:**
- Consumes: `ProfileSet::{backlight_config, set_backlight_slot_color, persist_backlight}`; `crate::led::Color`.

**Note:** egui UI — verified manually. Uses the existing deferred-action pattern (collect the change, apply after the loop) so no `&mut self` call happens inside an egui closure.

- [ ] **Step 1: Replace the slot loop with color-aware rows**

Replace the existing slot loop (the `for (i, m) in mkeys.iter().enumerate()` block that only renders `selectable_label` and sets `switch_to`) with:

```rust
        let mkeys = [MKey::M1, MKey::M2, MKey::M3];
        let mut switch_to: Option<MKey> = None;
        // Deferred: (slot index, new color or None for "use default").
        let mut slot_color_change: Option<(usize, Option<crate::led::Color>)> = None;
        let cfg = self.profiles.read().unwrap().backlight_config();
        for (i, m) in mkeys.iter().enumerate() {
            let label = match &slot_names[i] {
                Some(f) => format!("{m:?}  —  {}", display_of(f)),
                None => format!("{m:?}  —  (unassigned)"),
            };
            let mut use_default = cfg.slot_colors[i].is_none();
            let mut rgb = cfg.slot_colors[i]
                .map(|c| [c.0, c.1, c.2])
                .unwrap_or([cfg.default_color.0, cfg.default_color.1, cfg.default_color.2]);
            ui.horizontal(|ui| {
                if ui.selectable_label(*m == active, label).clicked() {
                    switch_to = Some(*m);
                }
                if ui.checkbox(&mut use_default, "default").changed() {
                    slot_color_change = Some((i, if use_default {
                        None
                    } else {
                        Some(crate::led::Color(rgb[0], rgb[1], rgb[2]))
                    }));
                }
                ui.add_enabled_ui(!use_default, |ui| {
                    if egui::color_picker::color_edit_button_srgb(ui, &mut rgb).changed() {
                        slot_color_change =
                            Some((i, Some(crate::led::Color(rgb[0], rgb[1], rgb[2]))));
                    }
                });
            });
        }
        if let Some(m) = switch_to {
            self.profiles.write().unwrap().set_active(m);
        }
        if let Some((i, c)) = slot_color_change {
            self.profiles.write().unwrap().set_backlight_slot_color(i, c);
            if let Err(e) = self.profiles.read().unwrap().persist_backlight() {
                log::warn!("persist backlight failed: {e:#}");
            }
        }
```

(Remove the now-duplicated `let mkeys`/`let mut switch_to`/`if let Some(m) = switch_to` lines that previously existed, so they are declared exactly once.)

- [ ] **Step 2: Build**

Run: `cargo build`
Expected: builds clean.

- [ ] **Step 3: Manual check**

Run: `cargo run` → Profiles tab. Each slot row shows a `default` checkbox and a color button (disabled when `default` is checked). Unchecking + picking a color writes `mN_color` into `config.toml`; re-checking removes it.

- [ ] **Step 4: Commit**

```bash
git add src/monitor/mod.rs
git commit -m "feat(gui): per-profile backlight color on the Profiles tab"
```

---

## Task 9: Config example, milestone update, hardware smoke test

**Files:**
- Modify: `config.toml` (commented `[backlight]` example)
- Move: `milestones/open/v0.5-rgb.md` → `milestones/ongoing/v0.5-rgb.md` (checklist + smoke test)

- [ ] **Step 1: Add a commented example to `config.toml`**

Append to `config.toml`:

```toml

# Keypad backlight (whole-pad single color; per-M-profile). Managed by the GUI
# (Settings + Profiles tabs); safe to edit by hand while stopped.
# [backlight]
# default_color  = "#FFFFFF"   # fallback for a profile with no color of its own
# brightness     = 1.0         # global 0.0-1.0 (0 = off)
# mkey_indicator = true        # light the active profile's M-key LED
# m1_color       = "#FF0000"   # optional per-slot overrides
# m2_color       = "#3030FF"
```

- [ ] **Step 2: Update the milestone and move it to ongoing**

Edit `milestones/open/v0.5-rgb.md`: set `Status: ongoing`, `Updated: 2026-07-16`, check the implemented task boxes, and add a smoke-test section:

```markdown
## Hardware smoke test (manual — no unit tests on the USB transfer)
- [ ] Backlight lights at the default color on startup.
- [ ] Switching M1/M2/M3 (device M-key AND GUI Profiles tab) changes the color per slot.
- [ ] A slot set to "use default" shows the global default color.
- [ ] Brightness slider dims the pad; 0% turns it off.
- [ ] The active profile's M-key LED lights (and only that one); toggling the
      Settings checkbox off turns the M-key LEDs off.
- [ ] Unplug/replug the G13 — the correct color/LEDs re-apply automatically.
- [ ] No G13 connected — app runs normally, no crash, warnings only.
```

Then move the file:

```bash
git mv milestones/open/v0.5-rgb.md milestones/ongoing/v0.5-rgb.md
```

- [ ] **Step 3: Full build + test**

Run: `cargo test && cargo build --release`
Expected: all tests pass; release binary builds clean.

- [ ] **Step 4: Commit**

```bash
git add config.toml milestones/
git commit -m "docs: backlight config example + v0.5 milestone to ongoing"
```

---

## Self-Review

**Spec coverage:**
- Config schema (`[backlight]`, hex, brightness, per-slot, defaults) → Tasks 1, 3, 4, 9. ✓
- `led` module (Color, BacklightConfig, LedState, resolve, packets) → Tasks 1, 2. ✓
- USB write path (shared cell, diff, control transfers, re-apply on reconnect, warn-and-continue) → Task 5. ✓
- Diff poller (150 ms, both runtimes) → Task 6. ✓
- GUI Settings (default color, brightness, M-key toggle) → Task 7. ✓
- GUI Profiles (per-slot color + use-default) → Task 8. ✓
- Persistence mirroring deadzone pattern → Task 4 (+ used in 7, 8). ✓
- Dry-run == active (LED driven regardless) → Tasks 5/6 wire the cell unconditionally; no dry-run gating. ✓
- Testing: pure unit tests → Tasks 1–4; manual smoke test → Task 9. ✓
- Out of scope (effects, per-key, LCD, MR, off-on-quit) → not implemented; MR handled in `resolve` (Task 2). ✓

**Placeholder scan:** No TBD/TODO; every code step has complete code. ✓

**Type consistency:** `Color`, `BacklightConfig`, `LedState`, `resolve`, `color_packet`/`mkey_packet`, `desired_led_state`, `backlight_config`, `set_backlight_*`, `persist_backlight`, `spawn_poller`, and `UsbReader::run(tx, desired)` names match across all tasks. ✓
