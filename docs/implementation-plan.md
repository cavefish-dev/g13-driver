# G13 Driver Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Windows console app in Rust that reads G-key presses from a Logitech G13 over USB and injects virtual keystrokes using a TOML config file.

**Architecture:** Five focused modules — `protocol` (USB report → events), `config` (TOML load + hot-reload), `dispatcher` (route events to injector), `injector` (platform-specific keystroke injection behind a trait), `usb` (raw USB read loop). Platform-specific code lives entirely inside `injector/windows.rs` behind `#[cfg(windows)]`.

**Tech Stack:** Rust 2021, `rusb 0.9` (libusb), `windows-sys 0.59` (SendInput), `serde + toml 0.8`, `notify 6`, `anyhow 1`, `env_logger 0.11`

**Prerequisites:**
- Rust toolchain installed (`rustup`)
- `libusb` available on Windows via the WinUSB driver (installed by Zadig — see Task 10)
- G13 physically connected

---

## File Map

| File | Responsibility |
|------|---------------|
| `Cargo.toml` | dependencies, target-gated windows-sys |
| `src/main.rs` | entry point: load config, spawn threads, event loop |
| `src/protocol.rs` | `G13Key` enum, `G13Event` enum, `ReportParser` |
| `src/config.rs` | `Config`, `RawConfig`, TOML load, `from_raw`, `get_binding` |
| `src/dispatcher.rs` | `Dispatcher`: routes `G13Event` → `KeyInjector` via config |
| `src/injector/mod.rs` | `KeyCombo`, `Modifier`, `KeyInjector` trait, `KeyCombo::parse` |
| `src/injector/key_map.rs` | `build_key_map()` — string → Win32 VKey lookup table |
| `src/injector/windows.rs` | `WindowsInjector` — calls `SendInput`, `#[cfg(windows)]` |
| `src/usb.rs` | `UsbReader`: opens G13, reads interrupt endpoint in a loop |
| `config.toml` | example key bindings shipped with the binary |
| `docs/zadig-setup.md` | one-time WinUSB driver swap guide |

---

## Task 1: Repository setup

**Files:**
- Create: `C:/repos/g13-driver/Cargo.toml`
- Create: `C:/repos/g13-driver/src/main.rs`

- [ ] **Step 1: Create the project directory**

```powershell
mkdir C:/repos/g13-driver
cd C:/repos/g13-driver
```

- [ ] **Step 2: Write Cargo.toml**

Create `C:/repos/g13-driver/Cargo.toml`:

```toml
[package]
name = "g13-driver"
version = "0.1.0"
edition = "2021"

[dependencies]
rusb    = "0.9"
serde   = { version = "1", features = ["derive"] }
toml    = "0.8"
anyhow  = "1"
log     = "0.4"
env_logger = "0.11"
notify  = "6"

[target.'cfg(windows)'.dependencies]
windows-sys = { version = "0.59", features = ["Win32_UI_Input_KeyboardAndMouse"] }
```

- [ ] **Step 3: Write stub main.rs**

Create `C:/repos/g13-driver/src/main.rs`:

```rust
fn main() {
    println!("g13-driver starting");
}
```

- [ ] **Step 4: Verify it compiles**

```powershell
cargo build
```

Expected: no errors, warning about unused function is fine.

- [ ] **Step 5: Initialize git**

```powershell
git init
git add Cargo.toml Cargo.lock src/main.rs
git commit -m "chore: scaffold g13-driver project"
```

---

## Task 2: G13 protocol types and report parser

**Files:**
- Create: `src/protocol.rs`
- Modify: `src/main.rs` (add `mod protocol;`)

- [ ] **Step 1: Write failing tests**

Create `src/protocol.rs` with tests only (no implementation yet):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn empty() -> [u8; 8] { [0u8; 8] }

    #[test]
    fn no_keys_no_events() {
        let mut p = ReportParser::new();
        assert!(p.parse(&empty()).is_empty());
    }

    #[test]
    fn g1_press() {
        let mut p = ReportParser::new();
        let mut r = empty();
        r[1] = 0b0000_0001;
        assert_eq!(p.parse(&r), vec![G13Event::KeyDown(G13Key::G1)]);
    }

    #[test]
    fn g1_release() {
        let mut p = ReportParser::new();
        let mut r = empty();
        r[1] = 0b0000_0001;
        p.parse(&r);
        assert_eq!(p.parse(&empty()), vec![G13Event::KeyUp(G13Key::G1)]);
    }

    #[test]
    fn g8_press() {
        let mut p = ReportParser::new();
        let mut r = empty();
        r[1] = 0b1000_0000;
        assert_eq!(p.parse(&r), vec![G13Event::KeyDown(G13Key::G8)]);
    }

    #[test]
    fn g9_press() {
        let mut p = ReportParser::new();
        let mut r = empty();
        r[2] = 0b0000_0001;
        assert_eq!(p.parse(&r), vec![G13Event::KeyDown(G13Key::G9)]);
    }

    #[test]
    fn g22_press() {
        let mut p = ReportParser::new();
        let mut r = empty();
        r[3] = 0b0010_0000;
        assert_eq!(p.parse(&r), vec![G13Event::KeyDown(G13Key::G22)]);
    }

    #[test]
    fn two_simultaneous_keys() {
        let mut p = ReportParser::new();
        let mut r = empty();
        r[1] = 0b0000_0011;
        let events = p.parse(&r);
        assert_eq!(events.len(), 2);
        assert!(events.contains(&G13Event::KeyDown(G13Key::G1)));
        assert!(events.contains(&G13Event::KeyDown(G13Key::G2)));
    }
}
```

- [ ] **Step 2: Add `mod protocol;` to main.rs and run tests to confirm they fail**

Add to top of `src/main.rs`:
```rust
mod protocol;
```

```powershell
cargo test
```

Expected: compile errors like `cannot find struct ReportParser` — tests exist but types don't yet.

- [ ] **Step 3: Implement protocol.rs**

Replace the contents of `src/protocol.rs` with:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum G13Key {
    G1,  G2,  G3,  G4,  G5,  G6,  G7,  G8,
    G9,  G10, G11, G12, G13, G14, G15, G16,
    G17, G18, G19, G20, G21, G22,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum G13Event {
    KeyDown(G13Key),
    KeyUp(G13Key),
}

pub struct ReportParser {
    prev_keys: u32,
}

impl ReportParser {
    pub fn new() -> Self {
        Self { prev_keys: 0 }
    }

    pub fn parse(&mut self, report: &[u8; 8]) -> Vec<G13Event> {
        let current = (report[1] as u32)
            | ((report[2] as u32) << 8)
            | ((report[3] as u32) << 16);

        let pressed  = current & !self.prev_keys;
        let released = self.prev_keys & !current;
        self.prev_keys = current;

        let mut events = Vec::new();
        for bit in 0..22u32 {
            if pressed  & (1 << bit) != 0 { events.push(G13Event::KeyDown(Self::bit_to_key(bit))); }
            if released & (1 << bit) != 0 { events.push(G13Event::KeyUp(Self::bit_to_key(bit))); }
        }
        events
    }

    fn bit_to_key(bit: u32) -> G13Key {
        match bit {
            0  => G13Key::G1,  1  => G13Key::G2,  2  => G13Key::G3,
            3  => G13Key::G4,  4  => G13Key::G5,  5  => G13Key::G6,
            6  => G13Key::G7,  7  => G13Key::G8,  8  => G13Key::G9,
            9  => G13Key::G10, 10 => G13Key::G11, 11 => G13Key::G12,
            12 => G13Key::G13, 13 => G13Key::G14, 14 => G13Key::G15,
            15 => G13Key::G16, 16 => G13Key::G17, 17 => G13Key::G18,
            18 => G13Key::G19, 19 => G13Key::G20, 20 => G13Key::G21,
            21 => G13Key::G22,
            _  => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty() -> [u8; 8] { [0u8; 8] }

    #[test]
    fn no_keys_no_events() {
        let mut p = ReportParser::new();
        assert!(p.parse(&empty()).is_empty());
    }

    #[test]
    fn g1_press() {
        let mut p = ReportParser::new();
        let mut r = empty();
        r[1] = 0b0000_0001;
        assert_eq!(p.parse(&r), vec![G13Event::KeyDown(G13Key::G1)]);
    }

    #[test]
    fn g1_release() {
        let mut p = ReportParser::new();
        let mut r = empty();
        r[1] = 0b0000_0001;
        p.parse(&r);
        assert_eq!(p.parse(&empty()), vec![G13Event::KeyUp(G13Key::G1)]);
    }

    #[test]
    fn g8_press() {
        let mut p = ReportParser::new();
        let mut r = empty();
        r[1] = 0b1000_0000;
        assert_eq!(p.parse(&r), vec![G13Event::KeyDown(G13Key::G8)]);
    }

    #[test]
    fn g9_press() {
        let mut p = ReportParser::new();
        let mut r = empty();
        r[2] = 0b0000_0001;
        assert_eq!(p.parse(&r), vec![G13Event::KeyDown(G13Key::G9)]);
    }

    #[test]
    fn g22_press() {
        let mut p = ReportParser::new();
        let mut r = empty();
        r[3] = 0b0010_0000;
        assert_eq!(p.parse(&r), vec![G13Event::KeyDown(G13Key::G22)]);
    }

    #[test]
    fn two_simultaneous_keys() {
        let mut p = ReportParser::new();
        let mut r = empty();
        r[1] = 0b0000_0011;
        let events = p.parse(&r);
        assert_eq!(events.len(), 2);
        assert!(events.contains(&G13Event::KeyDown(G13Key::G1)));
        assert!(events.contains(&G13Event::KeyDown(G13Key::G2)));
    }
}
```

- [ ] **Step 4: Run tests**

```powershell
cargo test protocol
```

Expected: `test result: ok. 7 passed`

- [ ] **Step 5: Commit**

```powershell
git add src/protocol.rs src/main.rs
git commit -m "feat: add G13Key, G13Event, ReportParser with bitmask decoding"
```

---

## Task 3: Injector types and trait

**Files:**
- Create: `src/injector/mod.rs`
- Modify: `src/main.rs` (add `mod injector;`)

- [ ] **Step 1: Write failing tests in `src/injector/mod.rs`**

Create directory `src/injector/` and write `src/injector/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_key() {
        let c = KeyCombo::parse("f5").unwrap();
        assert_eq!(c.key, "f5");
        assert!(c.modifiers.is_empty());
    }

    #[test]
    fn parse_ctrl_c() {
        let c = KeyCombo::parse("ctrl+c").unwrap();
        assert_eq!(c.key, "c");
        assert_eq!(c.modifiers, vec![Modifier::Ctrl]);
    }

    #[test]
    fn parse_shift_ctrl_esc() {
        let c = KeyCombo::parse("shift+ctrl+esc").unwrap();
        assert_eq!(c.key, "esc");
        assert!(c.modifiers.contains(&Modifier::Ctrl));
        assert!(c.modifiers.contains(&Modifier::Shift));
    }

    #[test]
    fn parse_is_case_insensitive() {
        let c = KeyCombo::parse("CTRL+C").unwrap();
        assert_eq!(c.key, "c");
        assert_eq!(c.modifiers, vec![Modifier::Ctrl]);
    }

    #[test]
    fn parse_windows_key() {
        let c = KeyCombo::parse("windows+d").unwrap();
        assert_eq!(c.key, "d");
        assert_eq!(c.modifiers, vec![Modifier::Windows]);
    }

    #[test]
    fn parse_no_key_is_error() {
        assert!(KeyCombo::parse("ctrl+shift").is_err());
    }
}
```

- [ ] **Step 2: Add `mod injector;` to main.rs and confirm tests fail**

Add to `src/main.rs`:
```rust
mod injector;
```

```powershell
cargo test injector
```

Expected: compile error — `KeyCombo`, `Modifier` not defined yet.

- [ ] **Step 3: Implement injector/mod.rs**

Replace contents of `src/injector/mod.rs` with:

```rust
pub mod key_map;
#[cfg(windows)]
pub mod windows;

use anyhow::{bail, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyCombo {
    pub modifiers: Vec<Modifier>,
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Modifier {
    Ctrl,
    Shift,
    Alt,
    Windows,
}

pub trait KeyInjector: Send + Sync {
    fn press(&self, combo: &KeyCombo) -> Result<()>;
}

impl KeyCombo {
    pub fn parse(s: &str) -> Result<Self> {
        let lower = s.to_lowercase();
        let parts: Vec<&str> = lower.split('+').map(str::trim).collect();
        let mut modifiers = Vec::new();
        let mut key: Option<String> = None;

        for part in &parts {
            match *part {
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

        let key = key.ok_or_else(|| anyhow::anyhow!("no key in combo: {}", s))?;
        Ok(Self { modifiers, key })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_key() {
        let c = KeyCombo::parse("f5").unwrap();
        assert_eq!(c.key, "f5");
        assert!(c.modifiers.is_empty());
    }

    #[test]
    fn parse_ctrl_c() {
        let c = KeyCombo::parse("ctrl+c").unwrap();
        assert_eq!(c.key, "c");
        assert_eq!(c.modifiers, vec![Modifier::Ctrl]);
    }

    #[test]
    fn parse_shift_ctrl_esc() {
        let c = KeyCombo::parse("shift+ctrl+esc").unwrap();
        assert_eq!(c.key, "esc");
        assert!(c.modifiers.contains(&Modifier::Ctrl));
        assert!(c.modifiers.contains(&Modifier::Shift));
    }

    #[test]
    fn parse_is_case_insensitive() {
        let c = KeyCombo::parse("CTRL+C").unwrap();
        assert_eq!(c.key, "c");
        assert_eq!(c.modifiers, vec![Modifier::Ctrl]);
    }

    #[test]
    fn parse_windows_key() {
        let c = KeyCombo::parse("windows+d").unwrap();
        assert_eq!(c.key, "d");
        assert_eq!(c.modifiers, vec![Modifier::Windows]);
    }

    #[test]
    fn parse_no_key_is_error() {
        assert!(KeyCombo::parse("ctrl+shift").is_err());
    }
}
```

- [ ] **Step 4: Create stubs for submodules** (required to compile before Tasks 4 and 7 fill them in)

Create `src/injector/key_map.rs`:
```rust
// stub — implemented in Task 4
use std::collections::HashMap;
pub type VKey = u16;
pub fn build_key_map() -> HashMap<String, VKey> { HashMap::new() }
```

Create `src/injector/windows.rs`:
```rust
// stub — implemented in Task 7
use anyhow::Result;
use super::{KeyCombo, KeyInjector};
pub struct WindowsInjector;
impl WindowsInjector { pub fn new() -> Self { Self } }
impl KeyInjector for WindowsInjector {
    fn press(&self, _combo: &KeyCombo) -> Result<()> { Ok(()) }
}
```

- [ ] **Step 5: Run tests**

```powershell
cargo test injector::tests
```

Expected: `test result: ok. 6 passed`

- [ ] **Step 6: Commit**

```powershell
git add src/injector/mod.rs src/injector/key_map.rs src/main.rs
git commit -m "feat: add KeyCombo, Modifier, KeyInjector trait"
```

---

## Task 4: Key map

**Files:**
- Modify: `src/injector/key_map.rs` (replace stub with full implementation)

- [ ] **Step 1: Write failing tests at the bottom of key_map.rs**

Replace `src/injector/key_map.rs` with:

```rust
use std::collections::HashMap;

pub type VKey = u16;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_a_to_z() {
        let m = build_key_map();
        assert_eq!(m["a"], 0x41);
        assert_eq!(m["z"], 0x5A);
        assert_eq!(m["m"], 0x4D);
    }

    #[test]
    fn digits_0_to_9() {
        let m = build_key_map();
        assert_eq!(m["0"], 0x30);
        assert_eq!(m["9"], 0x39);
    }

    #[test]
    fn function_keys() {
        let m = build_key_map();
        assert_eq!(m["f1"],  0x70);
        assert_eq!(m["f12"], 0x7B);
        assert_eq!(m["f24"], 0x87);
    }

    #[test]
    fn special_keys() {
        let m = build_key_map();
        assert_eq!(m["enter"],     0x0D);
        assert_eq!(m["esc"],       0x1B);
        assert_eq!(m["backspace"], 0x08);
        assert_eq!(m["delete"],    0x2E);
        assert_eq!(m["pageup"],    0x21);
        assert_eq!(m["up"],        0x26);
    }

    #[test]
    fn modifier_keys() {
        let m = build_key_map();
        assert_eq!(m["ctrl"],    0x11);
        assert_eq!(m["shift"],   0x10);
        assert_eq!(m["alt"],     0xA4);
        assert_eq!(m["windows"], 0x5B);
    }

    #[test]
    fn unknown_key_absent() {
        let m = build_key_map();
        assert!(!m.contains_key("xyzzy"));
    }
}
```

- [ ] **Step 2: Run tests to confirm they fail**

```powershell
cargo test key_map
```

Expected: fail — `build_key_map` returns empty HashMap, lookups fail.

- [ ] **Step 3: Implement build_key_map**

Replace contents of `src/injector/key_map.rs` with the full implementation:

```rust
use std::collections::HashMap;

pub type VKey = u16;

pub fn build_key_map() -> HashMap<String, VKey> {
    let mut m: HashMap<String, VKey> = HashMap::new();

    // a-z → VK_A (0x41) through VK_Z (0x5A)
    for c in b'a'..=b'z' {
        m.insert((c as char).to_string(), (c - b'a' + 0x41) as VKey);
    }
    // 0-9 → VK_0 (0x30) through VK_9 (0x39)
    for c in b'0'..=b'9' {
        m.insert((c as char).to_string(), (c - b'0' + 0x30) as VKey);
    }
    // F1-F24 → 0x70 through 0x87
    for i in 1u16..=24 {
        m.insert(format!("f{i}"), 0x6F + i);
    }

    let specials: &[(&str, VKey)] = &[
        ("enter",       0x0D), ("return",       0x0D),
        ("esc",         0x1B), ("escape",        0x1B),
        ("space",       0x20), ("tab",           0x09),
        ("backspace",   0x08), ("delete",        0x2E),
        ("insert",      0x2D), ("home",          0x24),
        ("end",         0x23), ("pageup",        0x21),
        ("pagedown",    0x22), ("up",            0x26),
        ("down",        0x28), ("left",          0x25),
        ("right",       0x27), ("capslock",      0x14),
        ("printscreen", 0x2C), ("pause",         0x13),
        ("numlock",     0x90), ("scrolllock",    0x91),
        ("ctrl",        0x11), ("control",       0x11),
        ("shift",       0x10), ("alt",           0xA4),
        ("windows",     0x5B), ("win",           0x5B),
    ];
    for (name, vk) in specials {
        m.insert(name.to_string(), *vk);
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_a_to_z() {
        let m = build_key_map();
        assert_eq!(m["a"], 0x41);
        assert_eq!(m["z"], 0x5A);
        assert_eq!(m["m"], 0x4D);
    }

    #[test]
    fn digits_0_to_9() {
        let m = build_key_map();
        assert_eq!(m["0"], 0x30);
        assert_eq!(m["9"], 0x39);
    }

    #[test]
    fn function_keys() {
        let m = build_key_map();
        assert_eq!(m["f1"],  0x70);
        assert_eq!(m["f12"], 0x7B);
        assert_eq!(m["f24"], 0x87);
    }

    #[test]
    fn special_keys() {
        let m = build_key_map();
        assert_eq!(m["enter"],     0x0D);
        assert_eq!(m["esc"],       0x1B);
        assert_eq!(m["backspace"], 0x08);
        assert_eq!(m["delete"],    0x2E);
        assert_eq!(m["pageup"],    0x21);
        assert_eq!(m["up"],        0x26);
    }

    #[test]
    fn modifier_keys() {
        let m = build_key_map();
        assert_eq!(m["ctrl"],    0x11);
        assert_eq!(m["shift"],   0x10);
        assert_eq!(m["alt"],     0xA4);
        assert_eq!(m["windows"], 0x5B);
    }

    #[test]
    fn unknown_key_absent() {
        let m = build_key_map();
        assert!(!m.contains_key("xyzzy"));
    }
}
```

- [ ] **Step 4: Run tests**

```powershell
cargo test key_map
```

Expected: `test result: ok. 6 passed`

- [ ] **Step 5: Commit**

```powershell
git add src/injector/key_map.rs
git commit -m "feat: add VKey lookup table for all G13-relevant keys"
```

---

## Task 5: Config loading

**Files:**
- Create: `src/config.rs`
- Modify: `src/main.rs` (add `mod config;`)

- [ ] **Step 1: Write failing tests in config.rs**

Create `src/config.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::G13Key;

    fn raw(pairs: &[(&str, &str)]) -> RawConfig {
        RawConfig {
            keys: pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        }
    }

    #[test]
    fn loads_bindings_from_raw() {
        let config = Config::from_raw(raw(&[("G1", "ctrl+c"), ("G2", "f5")])).unwrap();
        assert_eq!(config.get_binding(G13Key::G1), Some("ctrl+c"));
        assert_eq!(config.get_binding(G13Key::G2), Some("f5"));
    }

    #[test]
    fn unknown_g13_key_is_error() {
        assert!(Config::from_raw(raw(&[("G99", "ctrl+c")])).is_err());
    }

    #[test]
    fn unmapped_key_returns_none() {
        let config = Config::from_raw(raw(&[])).unwrap();
        assert_eq!(config.get_binding(G13Key::G5), None);
    }

    #[test]
    fn key_names_are_case_insensitive() {
        let config = Config::from_raw(raw(&[("g1", "ctrl+c")])).unwrap();
        assert_eq!(config.get_binding(G13Key::G1), Some("ctrl+c"));
    }

    #[test]
    fn parses_toml_content() {
        let src = r#"
[keys]
G1 = "ctrl+c"
G3 = "f5"
"#;
        let raw: RawConfig = toml::from_str(src).unwrap();
        let config = Config::from_raw(raw).unwrap();
        assert_eq!(config.get_binding(G13Key::G1), Some("ctrl+c"));
        assert_eq!(config.get_binding(G13Key::G3), Some("f5"));
        assert_eq!(config.get_binding(G13Key::G2), None);
    }
}
```

- [ ] **Step 2: Add `mod config;` to main.rs and confirm tests fail**

Add to `src/main.rs`:
```rust
mod config;
```

```powershell
cargo test config
```

Expected: compile error — `Config`, `RawConfig` not defined.

- [ ] **Step 3: Implement config.rs**

Replace contents of `src/config.rs` with:

```rust
use std::collections::HashMap;
use std::path::PathBuf;
use anyhow::{Context, Result};
use serde::Deserialize;
use crate::protocol::G13Key;

#[derive(Debug, Deserialize, Clone)]
pub struct RawConfig {
    #[serde(default)]
    pub keys: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct Config {
    key_bindings: HashMap<G13Key, String>,
}

impl Config {
    pub fn load(path: &PathBuf) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config: {}", path.display()))?;
        let raw: RawConfig = toml::from_str(&content)
            .with_context(|| format!("failed to parse config: {}", path.display()))?;
        Self::from_raw(raw)
    }

    pub(crate) fn from_raw(raw: RawConfig) -> Result<Self> {
        let mut key_bindings = HashMap::new();
        for (name, binding) in raw.keys {
            let key = parse_g13_key(&name)
                .with_context(|| format!("unknown G13 key: {}", name))?;
            key_bindings.insert(key, binding);
        }
        Ok(Self { key_bindings })
    }

    pub fn get_binding(&self, key: G13Key) -> Option<&str> {
        self.key_bindings.get(&key).map(|s| s.as_str())
    }
}

fn parse_g13_key(s: &str) -> Option<G13Key> {
    match s.to_uppercase().as_str() {
        "G1"  => Some(G13Key::G1),  "G2"  => Some(G13Key::G2),
        "G3"  => Some(G13Key::G3),  "G4"  => Some(G13Key::G4),
        "G5"  => Some(G13Key::G5),  "G6"  => Some(G13Key::G6),
        "G7"  => Some(G13Key::G7),  "G8"  => Some(G13Key::G8),
        "G9"  => Some(G13Key::G9),  "G10" => Some(G13Key::G10),
        "G11" => Some(G13Key::G11), "G12" => Some(G13Key::G12),
        "G13" => Some(G13Key::G13), "G14" => Some(G13Key::G14),
        "G15" => Some(G13Key::G15), "G16" => Some(G13Key::G16),
        "G17" => Some(G13Key::G17), "G18" => Some(G13Key::G18),
        "G19" => Some(G13Key::G19), "G20" => Some(G13Key::G20),
        "G21" => Some(G13Key::G21), "G22" => Some(G13Key::G22),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::G13Key;

    fn raw(pairs: &[(&str, &str)]) -> RawConfig {
        RawConfig {
            keys: pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        }
    }

    #[test]
    fn loads_bindings_from_raw() {
        let config = Config::from_raw(raw(&[("G1", "ctrl+c"), ("G2", "f5")])).unwrap();
        assert_eq!(config.get_binding(G13Key::G1), Some("ctrl+c"));
        assert_eq!(config.get_binding(G13Key::G2), Some("f5"));
    }

    #[test]
    fn unknown_g13_key_is_error() {
        assert!(Config::from_raw(raw(&[("G99", "ctrl+c")])).is_err());
    }

    #[test]
    fn unmapped_key_returns_none() {
        let config = Config::from_raw(raw(&[])).unwrap();
        assert_eq!(config.get_binding(G13Key::G5), None);
    }

    #[test]
    fn key_names_are_case_insensitive() {
        let config = Config::from_raw(raw(&[("g1", "ctrl+c")])).unwrap();
        assert_eq!(config.get_binding(G13Key::G1), Some("ctrl+c"));
    }

    #[test]
    fn parses_toml_content() {
        let src = r#"
[keys]
G1 = "ctrl+c"
G3 = "f5"
"#;
        let raw: RawConfig = toml::from_str(src).unwrap();
        let config = Config::from_raw(raw).unwrap();
        assert_eq!(config.get_binding(G13Key::G1), Some("ctrl+c"));
        assert_eq!(config.get_binding(G13Key::G3), Some("f5"));
        assert_eq!(config.get_binding(G13Key::G2), None);
    }
}
```

- [ ] **Step 4: Run tests**

```powershell
cargo test config
```

Expected: `test result: ok. 5 passed`

- [ ] **Step 5: Commit**

```powershell
git add src/config.rs src/main.rs
git commit -m "feat: add Config with TOML loading and G13Key mapping"
```

---

## Task 6: Dispatcher

**Files:**
- Create: `src/dispatcher.rs`
- Modify: `src/main.rs` (add `mod dispatcher;`)

- [ ] **Step 1: Write failing tests in dispatcher.rs**

Create `src/dispatcher.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, RawConfig};
    use crate::injector::Modifier;
    use crate::protocol::{G13Event, G13Key};
    use std::sync::{Arc, Mutex, RwLock};

    struct MockInjector(Arc<Mutex<Vec<KeyCombo>>>);

    impl MockInjector {
        fn new() -> (Self, Arc<Mutex<Vec<KeyCombo>>>) {
            let calls = Arc::new(Mutex::new(Vec::new()));
            (Self(calls.clone()), calls)
        }
    }

    impl KeyInjector for MockInjector {
        fn press(&self, combo: &KeyCombo) -> anyhow::Result<()> {
            self.0.lock().unwrap().push(combo.clone());
            Ok(())
        }
    }

    fn make_config(pairs: &[(&str, &str)]) -> Arc<RwLock<Config>> {
        let keys = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        Arc::new(RwLock::new(Config::from_raw(RawConfig { keys }).unwrap()))
    }

    #[test]
    fn key_down_triggers_injection() {
        let config = make_config(&[("G1", "ctrl+c")]);
        let (injector, calls) = MockInjector::new();
        let d = Dispatcher::new(config, Box::new(injector));

        d.handle(G13Event::KeyDown(G13Key::G1)).unwrap();

        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].key, "c");
        assert_eq!(calls[0].modifiers, vec![Modifier::Ctrl]);
    }

    #[test]
    fn key_up_is_ignored() {
        let config = make_config(&[("G1", "ctrl+c")]);
        let (injector, calls) = MockInjector::new();
        let d = Dispatcher::new(config, Box::new(injector));

        d.handle(G13Event::KeyUp(G13Key::G1)).unwrap();

        assert!(calls.lock().unwrap().is_empty());
    }

    #[test]
    fn unmapped_key_does_nothing() {
        let config = make_config(&[]);
        let (injector, calls) = MockInjector::new();
        let d = Dispatcher::new(config, Box::new(injector));

        d.handle(G13Event::KeyDown(G13Key::G5)).unwrap();

        assert!(calls.lock().unwrap().is_empty());
    }

    #[test]
    fn two_keys_dispatched_independently() {
        let config = make_config(&[("G1", "ctrl+c"), ("G2", "f5")]);
        let (injector, calls) = MockInjector::new();
        let d = Dispatcher::new(config, Box::new(injector));

        d.handle(G13Event::KeyDown(G13Key::G1)).unwrap();
        d.handle(G13Event::KeyDown(G13Key::G2)).unwrap();

        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[1].key, "f5");
        assert!(calls[1].modifiers.is_empty());
    }
}
```

- [ ] **Step 2: Add `mod dispatcher;` to main.rs and confirm tests fail**

Add to `src/main.rs`:
```rust
mod dispatcher;
```

```powershell
cargo test dispatcher
```

Expected: compile error — `Dispatcher` not defined.

- [ ] **Step 3: Implement dispatcher.rs**

Replace contents of `src/dispatcher.rs` with:

```rust
use anyhow::Result;
use std::sync::{Arc, RwLock};
use crate::config::Config;
use crate::injector::{KeyCombo, KeyInjector};
use crate::protocol::{G13Event, G13Key};

pub struct Dispatcher {
    config: Arc<RwLock<Config>>,
    injector: Box<dyn KeyInjector>,
}

impl Dispatcher {
    pub fn new(config: Arc<RwLock<Config>>, injector: Box<dyn KeyInjector>) -> Self {
        Self { config, injector }
    }

    pub fn handle(&self, event: G13Event) -> Result<()> {
        let G13Event::KeyDown(key) = event else { return Ok(()); };
        let binding = {
            let cfg = self.config.read().unwrap();
            cfg.get_binding(key).map(str::to_owned)
        };
        if let Some(binding) = binding {
            let combo = KeyCombo::parse(&binding)?;
            self.injector.press(&combo)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, RawConfig};
    use crate::injector::Modifier;
    use crate::protocol::{G13Event, G13Key};
    use std::sync::{Arc, Mutex, RwLock};

    struct MockInjector(Arc<Mutex<Vec<KeyCombo>>>);

    impl MockInjector {
        fn new() -> (Self, Arc<Mutex<Vec<KeyCombo>>>) {
            let calls = Arc::new(Mutex::new(Vec::new()));
            (Self(calls.clone()), calls)
        }
    }

    impl KeyInjector for MockInjector {
        fn press(&self, combo: &KeyCombo) -> anyhow::Result<()> {
            self.0.lock().unwrap().push(combo.clone());
            Ok(())
        }
    }

    fn make_config(pairs: &[(&str, &str)]) -> Arc<RwLock<Config>> {
        let keys = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        Arc::new(RwLock::new(Config::from_raw(RawConfig { keys }).unwrap()))
    }

    #[test]
    fn key_down_triggers_injection() {
        let config = make_config(&[("G1", "ctrl+c")]);
        let (injector, calls) = MockInjector::new();
        let d = Dispatcher::new(config, Box::new(injector));

        d.handle(G13Event::KeyDown(G13Key::G1)).unwrap();

        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].key, "c");
        assert_eq!(calls[0].modifiers, vec![Modifier::Ctrl]);
    }

    #[test]
    fn key_up_is_ignored() {
        let config = make_config(&[("G1", "ctrl+c")]);
        let (injector, calls) = MockInjector::new();
        let d = Dispatcher::new(config, Box::new(injector));

        d.handle(G13Event::KeyUp(G13Key::G1)).unwrap();

        assert!(calls.lock().unwrap().is_empty());
    }

    #[test]
    fn unmapped_key_does_nothing() {
        let config = make_config(&[]);
        let (injector, calls) = MockInjector::new();
        let d = Dispatcher::new(config, Box::new(injector));

        d.handle(G13Event::KeyDown(G13Key::G5)).unwrap();

        assert!(calls.lock().unwrap().is_empty());
    }

    #[test]
    fn two_keys_dispatched_independently() {
        let config = make_config(&[("G1", "ctrl+c"), ("G2", "f5")]);
        let (injector, calls) = MockInjector::new();
        let d = Dispatcher::new(config, Box::new(injector));

        d.handle(G13Event::KeyDown(G13Key::G1)).unwrap();
        d.handle(G13Event::KeyDown(G13Key::G2)).unwrap();

        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[1].key, "f5");
        assert!(calls[1].modifiers.is_empty());
    }
}
```

- [ ] **Step 4: Run tests**

```powershell
cargo test dispatcher
```

Expected: `test result: ok. 4 passed`

- [ ] **Step 5: Commit**

```powershell
git add src/dispatcher.rs src/main.rs
git commit -m "feat: add Dispatcher routing G13Events to KeyInjector via config"
```

---

## Task 7: Windows injector

**Files:**
- Create: `src/injector/windows.rs`

> **Note:** `SendInput` injects into the live OS input stack — no unit tests. Verification is manual (Step 4).

- [ ] **Step 1: Create `src/injector/windows.rs`**

```rust
use anyhow::{Context, Result};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
};
use super::{KeyCombo, KeyInjector, Modifier};
use super::key_map::build_key_map;
use std::collections::HashMap;

pub struct WindowsInjector {
    key_map: HashMap<String, u16>,
}

impl WindowsInjector {
    pub fn new() -> Self {
        Self { key_map: build_key_map() }
    }

    fn modifier_vk(m: &Modifier) -> u16 {
        match m {
            Modifier::Ctrl    => 0x11,
            Modifier::Shift   => 0x10,
            Modifier::Alt     => 0xA4,
            Modifier::Windows => 0x5B,
        }
    }

    fn make_input(vk: u16, flags: u32) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
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
}
```

- [ ] **Step 2: Verify it compiles**

```powershell
cargo build
```

Expected: no errors.

- [ ] **Step 3: Commit**

```powershell
git add src/injector/windows.rs
git commit -m "feat: add WindowsInjector using SendInput"
```

- [ ] **Step 4: Manual smoke test (do this after Task 9 wires main.rs)**

Open Notepad. Run:
```powershell
cargo run
```

Press G1 (mapped to `ctrl+c`) — nothing in focus yet. Switch to Notepad, type some text, press G1 — text should be copied. Press G2 (`ctrl+v`) — text should paste. Check log output for any `SendInput returned 0` warnings.

---

## Task 8: USB reader

**Files:**
- Create: `src/usb.rs`
- Modify: `src/main.rs` (add `mod usb;`)

> **Note:** `UsbReader` requires a physical G13 and WinUSB driver — no unit tests. Verification is manual (Step 4). Complete Task 10 (Zadig setup) before running.

- [ ] **Step 1: Create `src/usb.rs`**

```rust
use anyhow::{Context, Result};
use rusb::{DeviceHandle, GlobalContext};
use std::sync::mpsc::Sender;
use std::time::Duration;
use crate::protocol::{G13Event, ReportParser};

const G13_VID: u16 = 0x046D;
const G13_PID: u16 = 0xC21C;
const ENDPOINT_IN: u8 = 0x81;
const READ_TIMEOUT: Duration = Duration::from_millis(100);

pub struct UsbReader {
    handle: DeviceHandle<GlobalContext>,
}

impl UsbReader {
    pub fn open() -> Result<Self> {
        let handle = rusb::open_device_with_vid_pid(G13_VID, G13_PID)
            .context("G13 not found — plug it in and run Zadig to install WinUSB (see docs/zadig-setup.md)")?;
        handle.claim_interface(0)
            .context("failed to claim USB interface 0 — is another driver already attached?")?;
        Ok(Self { handle })
    }

    pub fn run(mut self, tx: Sender<G13Event>) -> Result<()> {
        let mut parser = ReportParser::new();
        let mut buf = [0u8; 8];
        loop {
            match self.handle.read_interrupt(ENDPOINT_IN, &mut buf, READ_TIMEOUT) {
                Ok(8) => {
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
}
```

- [ ] **Step 2: Add `mod usb;` to main.rs and verify it compiles**

Add to `src/main.rs`:
```rust
mod usb;
```

```powershell
cargo build
```

Expected: no errors.

- [ ] **Step 3: Commit**

```powershell
git add src/usb.rs src/main.rs
git commit -m "feat: add UsbReader opening G13 via rusb and emitting G13Events"
```

- [ ] **Step 4: Manual smoke test (after Task 9 wires main.rs)**

With G13 connected and WinUSB installed:
```powershell
$env:RUST_LOG="debug"; cargo run
```

Expected log output when pressing G1:
```
[DEBUG g13_driver::usb] read 8 bytes
[INFO  g13_driver] dispatching KeyDown(G1)
```

If you see `G13 not found`, the WinUSB driver is not installed — complete Task 10 first.

---

## Task 9: Wire main.rs

**Files:**
- Modify: `src/main.rs` (replace stub with full implementation)

- [ ] **Step 1: Replace main.rs with the full implementation**

Replace the entire contents of `src/main.rs`:

```rust
#[cfg(not(windows))]
compile_error!("g13-driver v0.1 targets Windows only; Linux support is planned for v1.0");

mod config;
mod dispatcher;
mod injector;
mod protocol;
mod usb;

use anyhow::Result;
use config::Config;
use std::path::PathBuf;
use std::sync::{mpsc, Arc, RwLock};
use std::thread;

fn main() -> Result<()> {
    env_logger::init();

    let config_path = PathBuf::from("config.toml");
    let config = Arc::new(RwLock::new(Config::load(&config_path)?));

    {
        let config = config.clone();
        let path = config_path.clone();
        thread::spawn(move || watch_config(config, path));
    }

    let (tx, rx) = mpsc::channel();
    let reader = usb::UsbReader::open()?;
    thread::spawn(move || {
        if let Err(e) = reader.run(tx) {
            log::error!("USB reader stopped: {e:#}");
        }
    });

    let injector = Box::new(injector::windows::WindowsInjector::new());
    let dispatcher = dispatcher::Dispatcher::new(config, injector);

    log::info!("g13-driver running — press Ctrl+C to stop");

    for event in rx {
        if let Err(e) = dispatcher.handle(event) {
            log::warn!("dispatch error: {e:#}");
        }
    }

    Ok(())
}

fn watch_config(config: Arc<RwLock<Config>>, path: PathBuf) {
    use notify::{Config as WatchConfig, Event, RecommendedWatcher, RecursiveMode, Watcher};

    let (tx, rx) = mpsc::channel::<notify::Result<Event>>();
    let mut watcher = match RecommendedWatcher::new(tx, WatchConfig::default()) {
        Ok(w) => w,
        Err(e) => { log::error!("failed to create file watcher: {e}"); return; }
    };
    if let Err(e) = watcher.watch(&path, RecursiveMode::NonRecursive) {
        log::error!("failed to watch {}: {e}", path.display());
        return;
    }
    for result in rx {
        if result.is_ok() {
            match Config::load(&path) {
                Ok(new) => {
                    *config.write().unwrap() = new;
                    log::info!("config reloaded");
                }
                Err(e) => log::warn!("config reload failed: {e:#}"),
            }
        }
    }
}
```

- [ ] **Step 2: Run all unit tests**

```powershell
cargo test
```

Expected: all tests pass (protocol, injector, config, dispatcher).

- [ ] **Step 3: Build release binary**

```powershell
cargo build --release
```

Expected: `target/release/g13-driver.exe` created, no errors.

- [ ] **Step 4: Commit**

```powershell
git add src/main.rs
git commit -m "feat: wire all components in main — USB reader, dispatcher, config hot-reload"
```

---

## Task 10: Example config and Zadig setup guide

**Files:**
- Create: `config.toml`
- Create: `docs/zadig-setup.md`

- [ ] **Step 1: Create config.toml**

Create `C:/repos/g13-driver/config.toml`:

```toml
# G13 key bindings — edit and save to hot-reload without restarting
#
# Keys G1–G22. Modifiers: ctrl, shift, alt, windows (also: control, win)
# Keys: a-z, 0-9, f1-f24, enter, esc, space, tab, backspace, delete,
#       insert, home, end, pageup, pagedown, up, down, left, right

[keys]
G1  = "ctrl+c"
G2  = "ctrl+v"
G3  = "ctrl+z"
G4  = "ctrl+shift+z"
G5  = "f5"
G6  = "alt+tab"
G7  = "windows+d"
G8  = "ctrl+alt+delete"
G9  = "ctrl+s"
G10 = "ctrl+a"
G11 = "ctrl+f"
G12 = "ctrl+w"
```

- [ ] **Step 2: Create docs/zadig-setup.md**

Create `C:/repos/g13-driver/docs/zadig-setup.md`:

```markdown
# Installing WinUSB for the G13 (one-time setup)

The G13 driver reads USB data directly via libusb. On Windows, this requires
replacing the default HID driver with WinUSB using Zadig. This is a one-time
step per machine.

## Steps

1. **Download Zadig** from https://zadig.akeo.ie/ — no installation needed.

2. **Plug in the G13** via USB.

3. **Run Zadig** as Administrator.

4. In the menu bar, click **Options → List All Devices**.

5. In the device dropdown, find **Logitech G13** (or a device with
   VID `046D`, PID `C21C`). If multiple G13 entries appear, select the
   one labelled "Interface 0".

6. In the driver box on the right, select **WinUSB**.

7. Click **Replace Driver** and wait for it to finish.

8. The G13 is now accessible to g13-driver. You only need to repeat this
   if you reinstall Windows or connect the G13 to a different physical USB
   port for the first time.

## Reverting

To restore the original HID driver (e.g., to use Logitech GHub again):
open Device Manager, find the G13 under "Universal Serial Bus devices",
right-click → **Update driver** → **Browse my computer** → **Let me pick**
→ select **HID-compliant game controller**.
```

- [ ] **Step 3: Verify end-to-end**

With G13 connected and WinUSB installed:
```powershell
$env:RUST_LOG="info"; cargo run
```

1. Log should show `g13-driver running`.
2. Open Notepad and type some text.
3. Press G1 — text should be copied (Ctrl+C).
4. Press G2 — text should paste (Ctrl+V).
5. Edit `config.toml`, change G5 from `"f5"` to `"ctrl+p"`, save.
6. Log should show `config reloaded`.
7. Press G5 — Print dialog should open.

- [ ] **Step 4: Commit**

```powershell
git add config.toml docs/zadig-setup.md
git commit -m "docs: add example config.toml and Zadig WinUSB setup guide"
```

---

## Spec coverage check

| Spec requirement | Task |
|-----------------|------|
| G-key press/release detection | Task 2 (ReportParser) |
| TOML config, flat string bindings | Task 5 (Config) |
| Key + modifier combo parsing | Task 3 (KeyCombo::parse) |
| Case-insensitive key names, modifier order irrelevant | Task 3 + Task 5 |
| Unmapped keys silently ignored | Task 5 + Task 6 |
| Win32 SendInput, single atomic call | Task 7 (WindowsInjector) |
| Hot-reload on config file change | Task 9 (watch_config) |
| Runs as console application | Task 9 (main.rs) |
| Platform trait isolating OS code | Task 3 (KeyInjector trait) |
| VID 0x046D / PID 0xC21C | Task 8 (UsbReader constants) |
| Zadig WinUSB setup documented | Task 10 |
| Example config shipped | Task 10 |
