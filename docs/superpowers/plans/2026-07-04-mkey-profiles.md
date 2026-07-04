# M-key Profiles Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Decode the G13 M-keys and switch between profiles — each profile a full binding set (`[keys]` + `[joystick]`) in its own file — with M1/M2/M3 selecting profiles and the GUI Profiles tab displaying/switching them.

**Architecture:** `config.toml` becomes a manifest naming a `profiles/` folder and the file bound to each M-key. Today's `Config` (a binding set) is renamed `Profile`; a new `ProfileSet` (the loaded profiles + active M-key) replaces `Config` behind the existing `Arc<RwLock>`. The dispatcher reads the active profile and switches it on `MKeyDown`; the GUI reads/sets the active slot.

**Tech Stack:** Rust, GNU toolchain (`stable-x86_64-pc-windows-gnu`), `eframe`/`egui`, `rusb`, `windows-sys`, `toml`/`serde`, `notify`, `log`. Build/test with `cargo` (PATH may need `export PATH="$HOME/.cargo/bin:/c/Strawberry/c/bin:$PATH"`).

## Global Constraints

- **Windows-only** (`src/main.rs:1-2`). OS injection behind `#[cfg(windows)]` in `src/injector/`. `protocol`/`config`/`device_state`/`joystick`/`monitor`/`runtime` stay platform-neutral (no Win32).
- **TDD** for pure logic (protocol M-keys, `Profile`/`Manifest`/`ProfileSet`, `DeviceState`, dispatcher switching). GUI rendering, USB/`SendInput`/wiring are manual-verify (documented exception) — verified by the hardware smoke test.
- **Hardware-verified M-key bits:** M1 = byte 6 bit 5 (`0x20`), M2 = byte 6 bit 6 (`0x40`), M3 = byte 6 bit 7 (`0x80`), MR = byte 7 bit 0 (`0x01`). Byte 7 bit 7 (heartbeat) and bit 3 (joystick click) are ignored.
- **Startup active = M1.** MR decoded but reserved (no-op). Switching to an empty slot is a no-op.
- **M1 required** (hard error if missing/invalid at startup). M2/M3 missing/invalid → empty slot (warn). Reload failures keep the last-good state (non-destructive), matching today's config-reload policy.
- **Legacy `config.toml`** (bare `[keys]`, no top-level `m1`) loads as a single M1 profile.
- **Release-held on profile switch** (a profile may rebind the joystick) — same safety valve as Dry-run/disconnect.
- **Error policy:** injection failures `log::warn!` and continue; no `panic!`/`unwrap()` in the runtime/consumer path (test code may `unwrap`; `mutex.lock().unwrap()` is the accepted poison-unreachable exception — see `milestones/finished/gui-monitor.md`).
- **Commits:** one per task; imperative subject; end every message with the trailer `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.
- Binary crate — `cargo test` (not `--lib`); focused: `cargo test <module>::`.

---

## File Structure

| File | Change | Responsibility |
|------|--------|----------------|
| `src/protocol.rs` | Modify | `MKey` enum; `MKeyDown`/`MKeyUp` events; parser decodes bytes 6/7 |
| `src/config.rs` | Modify | Rename `Config`→`Profile`; add `Manifest`, `ProfileSet` |
| `src/dispatcher.rs` | Modify | Read active profile; `MKeyDown` switches; release-held on switch |
| `src/runtime.rs` | Modify | Load `ProfileSet`; watch `profiles_dir`; wire `ProfileSet` |
| `src/monitor/mod.rs` | Modify | Header profile name; M-key indicator; real Profiles tab |
| `src/device_state.rs` | Modify | Track pressed M-keys |
| `src/main.rs` | Modify | `Arc<RwLock<ProfileSet>>` in mode selection |
| `config.toml` + `profiles/*.toml` | Create/Modify | Manifest + example profile files |

---

## Task 1: M-key decode in the parser

**Files:** Modify `src/protocol.rs`.

**Interfaces:**
- Produces: `pub enum MKey { M1, M2, M3, MR }` (derives `Debug, Clone, Copy, PartialEq, Eq, Hash`); `G13Event::MKeyDown(MKey)`, `G13Event::MKeyUp(MKey)`.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `src/protocol.rs`:

```rust
    #[test]
    fn m1_press_and_release() {
        let mut p = ReportParser::new();
        let mut r = idle();
        r[6] = 0x20; // M1 = byte 6 bit 5
        assert_eq!(p.parse(&r), vec![G13Event::MKeyDown(MKey::M1)]);
        assert_eq!(p.parse(&idle()), vec![G13Event::MKeyUp(MKey::M1)]);
    }

    #[test]
    fn m2_m3_and_mr_press() {
        let mut p = ReportParser::new();
        let mut r = idle();
        r[6] = 0x40; // M2
        assert_eq!(p.parse(&r), vec![G13Event::MKeyDown(MKey::M2)]);
        let mut r = idle();
        r[6] = 0x80; // M3 (byte 6 bit 7)
        // transition from M2-held to M3-held: M2 up, M3 down
        let ev = p.parse(&r);
        assert!(ev.contains(&G13Event::MKeyUp(MKey::M2)));
        assert!(ev.contains(&G13Event::MKeyDown(MKey::M3)));
        let mut r = idle();
        r[7] = 0x01; // MR = byte 7 bit 0
        let ev = p.parse(&r);
        assert!(ev.contains(&G13Event::MKeyUp(MKey::M3)));
        assert!(ev.contains(&G13Event::MKeyDown(MKey::MR)));
    }

    #[test]
    fn byte7_heartbeat_and_click_ignored() {
        let mut p = ReportParser::new();
        let mut r = idle();
        r[7] = 0x88; // bit7 heartbeat + bit3 joystick click — neither is an M-key
        assert!(p.parse(&r).is_empty());
    }

    #[test]
    fn mkey_and_gkey_together() {
        let mut p = ReportParser::new();
        let mut r = idle();
        r[3] = 0b0000_0001; // G1
        r[6] = 0x20;        // M1
        let ev = p.parse(&r);
        assert!(ev.contains(&G13Event::KeyDown(G13Key::G1)));
        assert!(ev.contains(&G13Event::MKeyDown(MKey::M1)));
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test protocol:: 2>&1 | tail -15`
Expected: FAIL — `cannot find type MKey` / `no variant MKeyDown`.

- [ ] **Step 3: Implement**

In `src/protocol.rs`, add the enum after `G13Key` (before `G13Event`):

```rust
/// The mode/profile keys above the LCD. M1-M3 select profiles; MR is reserved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MKey {
    M1,
    M2,
    M3,
    MR,
}
```

Add the two variants to `G13Event`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum G13Event {
    KeyDown(G13Key),
    KeyUp(G13Key),
    JoystickMove { x: u8, y: u8 },
    MKeyDown(MKey),
    MKeyUp(MKey),
}
```

Add `prev_mkeys: u8` to `ReportParser`:

```rust
pub struct ReportParser {
    prev_keys: u32,
    prev_x: u8,
    prev_y: u8,
    prev_mkeys: u8,
}

impl ReportParser {
    pub fn new() -> Self {
        Self { prev_keys: 0, prev_x: 127, prev_y: 127, prev_mkeys: 0 }
    }
```

In `parse`, after the G-key loop and before `events`, add M-key edge detection (M-keys packed into a nibble: bit0=M1, bit1=M2, bit2=M3, bit3=MR):

```rust
        // M-keys: byte 6 bits 5-7 (M1,M2,M3) and byte 7 bit 0 (MR). Byte 7 bit 7
        // (heartbeat) and bit 3 (joystick click) are not M-keys. Packed to a nibble.
        let current_m = (u8::from(report[6] & 0x20 != 0))
            | (u8::from(report[6] & 0x40 != 0) << 1)
            | (u8::from(report[6] & 0x80 != 0) << 2)
            | (u8::from(report[7] & 0x01 != 0) << 3);
        let m_pressed = current_m & !self.prev_mkeys;
        let m_released = self.prev_mkeys & !current_m;
        self.prev_mkeys = current_m;
        for bit in 0..4u8 {
            let mkey = Self::bit_to_mkey(bit);
            if m_pressed & (1 << bit) != 0 { events.push(G13Event::MKeyDown(mkey)); }
            if m_released & (1 << bit) != 0 { events.push(G13Event::MKeyUp(mkey)); }
        }
```

Add the helper next to `bit_to_key`:

```rust
    fn bit_to_mkey(bit: u8) -> MKey {
        match bit {
            0 => MKey::M1,
            1 => MKey::M2,
            2 => MKey::M3,
            3 => MKey::MR,
            _ => unreachable!(),
        }
    }
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test protocol:: 2>&1 | tail -5`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/protocol.rs
git commit -m "feat: decode M-keys (M1/M2/M3/MR) from report bytes 6,7

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Rename Config → Profile (isolated via alias)

**Files:** Modify `src/config.rs`.

**Interfaces:**
- Produces: `Profile` (was `Config`) with unchanged API `Profile::load(&PathBuf) -> Result<Profile>`, `Profile::from_raw`, `get_binding`, `joystick`. A temporary `pub type Config = Profile;` keeps consumers compiling until Task 4.

- [ ] **Step 1: Rename the struct and its impl/tests in `config.rs`**

In `src/config.rs`: rename the struct `Config` to `Profile` and its `impl Config` to `impl Profile`. Update the doc/return types (`load` returns `Result<Self>` — no text change needed). In the `#[cfg(test)] mod tests`, replace every `Config::from_raw` with `Profile::from_raw` and every `Config::` with `Profile::`. Leave `RawConfig`, `JoystickConfig`, `JoystickMode` names as-is.

Concretely, the struct + impl header become:

```rust
#[derive(Debug, Clone)]
pub struct Profile {
    key_bindings: HashMap<G13Key, String>,
    joystick: Option<JoystickConfig>,
}

impl Profile {
    pub fn load(path: &PathBuf) -> Result<Self> {
```

- [ ] **Step 2: Add the compatibility alias**

At the end of the non-test part of `src/config.rs` (after the `impl Profile` block), add:

```rust
/// Temporary alias so existing consumers keep compiling while the profile
/// layer is introduced. Removed in the ProfileSet wiring task.
pub type Config = Profile;
```

- [ ] **Step 3: Build + test — no behavior change**

Run: `cargo test 2>&1 | tail -5`
Expected: `test result: ok. <N> passed` (same count as before this task; the alias means `dispatcher`/`runtime`/`monitor` are unchanged).
Run: `cargo build 2>&1 | tail -2` → `Finished`.

- [ ] **Step 4: Commit**

```bash
git add src/config.rs
git commit -m "refactor: rename Config to Profile (alias kept for consumers)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: Manifest + ProfileSet

**Files:** Modify `src/config.rs`.

**Interfaces:**
- Consumes: `Profile` (Task 2), `MKey` (Task 1).
- Produces:
  - `pub struct ProfileSet` with `ProfileSet::load(config_path: &Path) -> Result<ProfileSet>`, `active_profile(&self) -> &Profile`, `set_active(&mut self, MKey) -> bool`, `active(&self) -> MKey`, `name(&self, MKey) -> Option<&str>`, `active_name(&self) -> Option<&str>`, `available(&self) -> Vec<String>`.

- [ ] **Step 1: Write the failing tests**

Add a second test module at the end of `src/config.rs` (a dedicated module keeps the file-IO helpers together):

```rust
#[cfg(test)]
mod profileset_tests {
    use super::*;
    use crate::protocol::MKey;

    fn write(dir: &std::path::Path, name: &str, body: &str) {
        std::fs::write(dir.join(name), body).unwrap();
    }

    // Build a temp dir under the OS temp with a unique suffix from the test name.
    fn tmp(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("g13-test-{tag}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("profiles")).unwrap();
        d
    }

    #[test]
    fn loads_manifest_and_switches() {
        let d = tmp("manifest");
        write(&d.join("profiles"), "default.toml", "[keys]\nG1 = \"ctrl+c\"\n");
        write(&d.join("profiles"), "game.toml", "[keys]\nG1 = \"space\"\n");
        write(&d, "config.toml", "profiles_dir = \"profiles\"\nm1 = \"default.toml\"\nm2 = \"game.toml\"\n");

        let mut set = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert_eq!(set.active(), MKey::M1);
        assert_eq!(set.active_profile().get_binding(crate::protocol::G13Key::G1), Some("ctrl+c"));
        assert_eq!(set.name(MKey::M2), Some("game.toml"));

        assert!(set.set_active(MKey::M2));
        assert_eq!(set.active_profile().get_binding(crate::protocol::G13Key::G1), Some("space"));

        // M3 unbound -> no-op switch, stays on M2.
        assert!(!set.set_active(MKey::M3));
        assert_eq!(set.active(), MKey::M2);
        // MR reserved -> no-op.
        assert!(!set.set_active(MKey::MR));
    }

    #[test]
    fn legacy_config_is_single_m1_profile() {
        let d = tmp("legacy");
        write(&d, "config.toml", "[keys]\nG1 = \"ctrl+c\"\n");
        let set = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert_eq!(set.active_profile().get_binding(crate::protocol::G13Key::G1), Some("ctrl+c"));
        assert!(set.name(MKey::M2).is_none());
    }

    #[test]
    fn missing_m1_is_error() {
        let d = tmp("missing-m1");
        write(&d, "config.toml", "profiles_dir = \"profiles\"\nm1 = \"nope.toml\"\n");
        assert!(ProfileSet::load(&d.join("config.toml")).is_err());
    }

    #[test]
    fn available_lists_toml_files() {
        let d = tmp("available");
        write(&d.join("profiles"), "default.toml", "[keys]\n");
        write(&d.join("profiles"), "extra.toml", "[keys]\n");
        write(&d, "config.toml", "profiles_dir = \"profiles\"\nm1 = \"default.toml\"\n");
        let set = ProfileSet::load(&d.join("config.toml")).unwrap();
        let mut avail = set.available();
        avail.sort();
        assert_eq!(avail, vec!["default.toml".to_string(), "extra.toml".to_string()]);
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test profileset_tests:: 2>&1 | tail -15`
Expected: FAIL — `cannot find type ProfileSet`.

- [ ] **Step 3: Implement `Manifest` + `ProfileSet`**

Add to `src/config.rs` (after the `Profile` impl, before the `Config` alias). Add `use std::path::Path;` to the imports at the top (keep the existing `use std::path::PathBuf;`).

```rust
use crate::protocol::MKey;

#[derive(Debug, Deserialize)]
struct RawManifest {
    profiles_dir: Option<String>,
    m1: Option<String>,
    m2: Option<String>,
    m3: Option<String>,
}

/// The loaded profiles plus which M-key is active. Replaces a bare `Profile`
/// as the shared state so both the dispatcher and the GUI see profiles + active.
#[derive(Debug, Clone)]
pub struct ProfileSet {
    profiles_dir: PathBuf,
    m1: Profile,
    m2: Option<Profile>,
    m3: Option<Profile>,
    m1_name: Option<String>,
    m2_name: Option<String>,
    m3_name: Option<String>,
    active: MKey,
}

impl ProfileSet {
    /// Load from the manifest at `config_path`. Manifest mode when a top-level
    /// `m1` is present; otherwise the file is itself the single M1 profile
    /// (legacy). Paths resolve relative to the config file's directory.
    pub fn load(config_path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(config_path)
            .with_context(|| format!("failed to read config: {}", config_path.display()))?;
        let raw: RawManifest = toml::from_str(&content)
            .with_context(|| format!("failed to parse config: {}", config_path.display()))?;
        let base = config_path.parent().unwrap_or_else(|| Path::new("."));

        if let Some(m1_name) = raw.m1 {
            // Manifest mode.
            let dir = base.join(raw.profiles_dir.as_deref().unwrap_or("profiles"));
            let m1 = Profile::load(&dir.join(&m1_name))
                .with_context(|| format!("failed to load M1 profile {m1_name}"))?;
            let load_opt = |name: &Option<String>| -> (Option<Profile>, Option<String>) {
                match name {
                    Some(n) => match Profile::load(&dir.join(n)) {
                        Ok(p) => (Some(p), Some(n.clone())),
                        Err(e) => { log::warn!("skipping profile {n}: {e:#}"); (None, None) }
                    },
                    None => (None, None),
                }
            };
            let (m2, m2_name) = load_opt(&raw.m2);
            let (m3, m3_name) = load_opt(&raw.m3);
            Ok(Self {
                profiles_dir: dir,
                m1, m2, m3,
                m1_name: Some(m1_name),
                m2_name, m3_name,
                active: MKey::M1,
            })
        } else {
            // Legacy: the config file is a single profile.
            let m1 = Profile::load(&config_path.to_path_buf())?;
            let name = config_path.file_name().and_then(|s| s.to_str()).map(String::from);
            Ok(Self {
                profiles_dir: base.to_path_buf(),
                m1, m2: None, m3: None,
                m1_name: name, m2_name: None, m3_name: None,
                active: MKey::M1,
            })
        }
    }

    pub fn active(&self) -> MKey { self.active }

    pub fn active_profile(&self) -> &Profile {
        match self.active {
            MKey::M2 => self.m2.as_ref().unwrap_or(&self.m1),
            MKey::M3 => self.m3.as_ref().unwrap_or(&self.m1),
            _ => &self.m1,
        }
    }

    /// Switch the active profile. No-op (returns false) for MR or an empty slot.
    pub fn set_active(&mut self, k: MKey) -> bool {
        let ok = match k {
            MKey::M1 => true,
            MKey::M2 => self.m2.is_some(),
            MKey::M3 => self.m3.is_some(),
            MKey::MR => false,
        };
        if ok { self.active = k; }
        ok
    }

    pub fn name(&self, k: MKey) -> Option<&str> {
        match k {
            MKey::M1 => self.m1_name.as_deref(),
            MKey::M2 => self.m2_name.as_deref(),
            MKey::M3 => self.m3_name.as_deref(),
            MKey::MR => None,
        }
    }

    pub fn active_name(&self) -> Option<&str> { self.name(self.active) }

    /// All `.toml` files in the profiles folder (for the GUI browse list).
    pub fn available(&self) -> Vec<String> {
        let mut names = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.profiles_dir) {
            for e in entries.flatten() {
                if let Some(n) = e.file_name().to_str() {
                    if n.ends_with(".toml") { names.push(n.to_string()); }
                }
            }
        }
        names
    }
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test profileset_tests:: 2>&1 | tail -8`
Expected: PASS (4 tests).
Run: `cargo test 2>&1 | tail -3` → full suite green.

- [ ] **Step 5: Commit**

```bash
git add src/config.rs
git commit -m "feat: add Manifest and ProfileSet (profiles folder + active slot)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: Wire ProfileSet through dispatcher, runtime, monitor, main

**Files:** Modify `src/dispatcher.rs`, `src/runtime.rs`, `src/monitor/mod.rs`, `src/main.rs`, `src/config.rs` (remove the `Config` alias).

**Interfaces:**
- Consumes: `ProfileSet` (Task 3), `MKey`/`MKeyDown` (Task 1).
- Produces: `Dispatcher::new(profiles: Arc<RwLock<ProfileSet>>, injector)`; `runtime::load_config_and_watch(path) -> Result<Arc<RwLock<ProfileSet>>>`; `runtime::run_headless(profiles: Arc<RwLock<ProfileSet>>, rx)`.

- [ ] **Step 1: Update the dispatcher tests (new expectations)**

In `src/dispatcher.rs` test module, replace the `use` line `use crate::config::{Config, RawConfig};` with `use crate::config::ProfileSet;` and replace the `make_config` / `config_with_joystick` helpers with `ProfileSet`-based ones that write temp profile files. Add a switching test. Replace the helpers and add tests:

```rust
    use crate::protocol::MKey;

    fn write(p: &std::path::Path, body: &str) { std::fs::write(p, body).unwrap(); }

    fn profiles_two() -> Arc<RwLock<ProfileSet>> {
        let d = std::env::temp_dir().join("g13-disp-two");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("profiles")).unwrap();
        write(&d.join("profiles/default.toml"), "[keys]\nG1 = \"ctrl+c\"\n");
        write(&d.join("profiles/game.toml"), "[keys]\nG1 = \"space\"\n[joystick]\nup=\"w\"\n");
        write(&d.join("config.toml"), "profiles_dir=\"profiles\"\nm1=\"default.toml\"\nm2=\"game.toml\"\n");
        Arc::new(RwLock::new(ProfileSet::load(&d.join("config.toml")).unwrap()))
    }

    #[test]
    fn mkey_switches_active_profile() {
        let (injector, _calls) = MockInjector::new();
        let mut d = Dispatcher::new(profiles_two(), Box::new(injector));
        // On M1, G1 -> ctrl+c (a combo press). Switch to M2, G1 -> space.
        d.handle(G13Event::MKeyDown(MKey::M2)).unwrap();
        // Verify by dispatching G1 and checking the injected combo.
        let (injector, calls) = MockInjector::new();
        let mut d = Dispatcher::new(profiles_two(), Box::new(injector));
        d.handle(G13Event::MKeyDown(MKey::M2)).unwrap();
        d.handle(G13Event::KeyDown(G13Key::G1)).unwrap();
        assert_eq!(calls.lock().unwrap()[0].key, "space");
    }

    #[test]
    fn mkey_switch_releases_held_joystick() {
        let (injector, holds) = MockInjector::new_with_holds();
        let mut d = Dispatcher::new(profiles_two(), Box::new(injector));
        // With M2 (has joystick up=w) active, hold up so a key is held.
        d.handle(G13Event::MKeyDown(MKey::M2)).unwrap();
        d.handle(G13Event::JoystickMove { x: 127, y: 0 }).unwrap(); // hold "w"
        holds.lock().unwrap().clear();
        // Switch back to M1 -> release_held fires before the switch.
        d.handle(G13Event::MKeyDown(MKey::M1)).unwrap();
        assert!(holds.lock().unwrap().iter().any(|s| s == "up:w"));
    }
```

Also: the existing dispatcher tests build a config via `make_config(&[...])` / `config_with_joystick()`. Replace those helpers so they return a single-profile `ProfileSet` (legacy-style). Replace `make_config`:

```rust
    fn make_config(pairs: &[(&str, &str)]) -> Arc<RwLock<ProfileSet>> {
        let d = std::env::temp_dir().join("g13-disp-single");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let mut body = String::from("[keys]\n");
        for (k, v) in pairs { body.push_str(&format!("{k} = \"{v}\"\n")); }
        write(&d.join("config.toml"), &body);
        Arc::new(RwLock::new(ProfileSet::load(&d.join("config.toml")).unwrap()))
    }
```

And replace `config_with_joystick()` similarly to write a `[keys]`+`[joystick]` legacy config and load it as a `ProfileSet`. (The joystick tests then pass through `active_profile().joystick()`.)

- [ ] **Step 2: Run to verify failure**

Run: `cargo test dispatcher:: 2>&1 | tail -20`
Expected: FAIL — `Dispatcher::new` still takes `Arc<RwLock<Config>>`; `MKeyDown` not handled; helpers changed.

- [ ] **Step 3: Update the dispatcher implementation**

In `src/dispatcher.rs`, change the imports and struct/handlers. Replace lines 1-58 (imports through `handle_joystick`) with:

```rust
use anyhow::Result;
use std::sync::{Arc, RwLock};
use crate::config::{JoystickMode, ProfileSet};
use crate::injector::{KeyCombo, KeyInjector};
use crate::joystick::{HoldAction, JoystickMapper};
use crate::protocol::{G13Event, G13Key, MKey};

pub struct Dispatcher {
    profiles: Arc<RwLock<ProfileSet>>,
    injector: Box<dyn KeyInjector>,
    joystick: JoystickMapper,
}

impl Dispatcher {
    pub fn new(profiles: Arc<RwLock<ProfileSet>>, injector: Box<dyn KeyInjector>) -> Self {
        Self { profiles, injector, joystick: JoystickMapper::new() }
    }

    pub fn handle(&mut self, event: G13Event) -> Result<()> {
        match event {
            G13Event::KeyDown(key) => self.handle_key(key)?,
            G13Event::KeyUp(_) => {}
            G13Event::JoystickMove { x, y } => self.handle_joystick(x, y),
            G13Event::MKeyDown(m) => self.handle_mkey(m),
            G13Event::MKeyUp(_) => {}
        }
        Ok(())
    }

    fn handle_key(&self, key: G13Key) -> Result<()> {
        let binding = {
            let set = self.profiles.read().unwrap();
            set.active_profile().get_binding(key).map(str::to_owned)
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
        // Read the active profile's joystick config live; clone so the guard is
        // dropped before we touch the injector.
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

    /// Switch profile on M1/M2/M3. Release held joystick keys first (a new
    /// profile may rebind the stick). MR is reserved.
    fn handle_mkey(&mut self, m: MKey) {
        if m == MKey::MR { return; }
        self.release_held();
        let mut set = self.profiles.write().unwrap();
        if set.set_active(m) {
            log::info!("profile -> {}", set.name(m).unwrap_or("?"));
        } else {
            log::warn!("no profile bound to {m:?}");
        }
    }
```

(The `apply` and `release_held` methods below line 58 are unchanged.)

- [ ] **Step 4: Update runtime, monitor, main, and remove the alias**

In `src/config.rs`, delete the `pub type Config = Profile;` line.

In `src/runtime.rs`: change the signatures and body to use `ProfileSet`:
- `use crate::config::{JoystickMode, ProfileSet};` (replace the `Config` import).
- `load_config_and_watch(path: PathBuf) -> Result<Arc<RwLock<ProfileSet>>>` — build via `ProfileSet::load(&path)?` wrapped in `Arc::new(RwLock::new(...))`, and change the watcher to also watch the profiles dir: after `watcher.watch(&path, RecursiveMode::NonRecursive)`, also watch the active profiles dir. Since the dir is inside the ProfileSet, capture it before spawning: read `set.available()`'s directory via a new `pub fn profiles_dir(&self) -> &Path` accessor — add that accessor to `ProfileSet` in `config.rs`:
  ```rust
  pub fn profiles_dir(&self) -> &std::path::Path { &self.profiles_dir }
  ```
  Then in `load_config_and_watch`, watch that dir recursively (ignore errors if it equals the config dir):
  ```rust
  let dir = config.read().unwrap().profiles_dir().to_path_buf();
  ```
  and pass both `path` and `dir` into `watch_config`.
- `watch_config(config: Arc<RwLock<ProfileSet>>, config_path: PathBuf, profiles_dir: PathBuf)` — watch both paths; on any event, reload preserving the active slot:
  ```rust
  let active = config.read().unwrap().active();
  match ProfileSet::load(&config_path) {
      Ok(mut new) => { new.set_active(active); *config.write().unwrap() = new; log::info!("config reloaded"); }
      Err(e) => log::warn!("config reload failed: {e:#}"),
  }
  ```
- `run_headless(config: Arc<RwLock<ProfileSet>>, rx)` — the mouse-mode check becomes `config.read().unwrap().active_profile().joystick()`.

In `src/monitor/mod.rs`: change the `config: Arc<RwLock<Config>>` field and all `self.config` reads to `profiles: Arc<RwLock<ProfileSet>>`. Where the render currently does `let cfg = self.config.read().unwrap();` and calls `cfg.get_binding(...)` / `cfg.joystick()`, insert `let set = self.profiles.read().unwrap(); let cfg = set.active_profile();`. Update `run`, `MonitorApp` fields, `MonitorApp::new`, and `start_consumer` (`Dispatcher::new(self.profiles.clone(), injector)`). Update the `import` `use crate::config::Config;` → `use crate::config::ProfileSet;`.

In `src/main.rs`: `runtime::load_config_and_watch` now returns `Arc<RwLock<ProfileSet>>` — the local `config` variable type follows automatically; no signature change needed, but update any `config::JoystickMode` references if present (they stay valid).

- [ ] **Step 5: Run to verify pass**

Run: `cargo test 2>&1 | tail -6`
Expected: PASS — dispatcher switching tests + all prior tests green.
Run: `cargo build 2>&1 | tail -2` → `Finished`.

- [ ] **Step 6: Commit**

```bash
git add src/config.rs src/dispatcher.rs src/runtime.rs src/monitor/mod.rs src/main.rs
git commit -m "feat: drive dispatch from active profile; M-keys switch profiles

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: GUI — profile display, M-key indicator, real Profiles tab

**Files:** Modify `src/device_state.rs`, `src/monitor/mod.rs`.

**Interfaces:**
- Consumes: `ProfileSet` (Task 3), `MKey` (Task 1).
- Produces: `DeviceState.mkeys: HashSet<MKey>`.

- [ ] **Step 1: DeviceState M-key tracking (TDD)**

In `src/device_state.rs`, add a failing test:

```rust
    #[test]
    fn mkey_down_and_up_tracked() {
        use crate::protocol::MKey;
        let mut s = DeviceState::new();
        s.apply(&G13Event::MKeyDown(MKey::M2));
        assert!(s.mkeys.contains(&MKey::M2));
        s.apply(&G13Event::MKeyUp(MKey::M2));
        assert!(!s.mkeys.contains(&MKey::M2));
    }
```

Run: `cargo test device_state:: 2>&1 | tail -8` → FAIL (`no field mkeys`).

Then add the field and handling. Change the struct + `Default` + `apply`:

```rust
use crate::protocol::{G13Event, G13Key, MKey};
// ... in the struct:
    pub mkeys: HashSet<MKey>,
// ... in Default::default(), add:
            mkeys: HashSet::new(),
// ... in apply(), add arms:
            G13Event::MKeyDown(m) => { self.mkeys.insert(*m); }
            G13Event::MKeyUp(m) => { self.mkeys.remove(m); }
```

Run: `cargo test device_state:: 2>&1 | tail -5` → PASS.

- [ ] **Step 2: Header shows the active profile name**

In `src/monitor/mod.rs`, in the top panel `update`, after the connection label, add the active profile name. Where the header renders (inside the `TopBottomPanel::top("hd")` horizontal), add after the connection match:

```rust
                if let Some(name) = self.profiles.read().unwrap().active_name() {
                    ui.separator();
                    ui.label(format!("Profile: {name}"));
                }
```

- [ ] **Step 3: M-key indicator row on the Monitor tab**

In `render_monitor`, after the joystick block (inside the centered vertical, or below the grid), add an M-key row. At the end of `render_monitor` (before the method's closing brace), add:

```rust
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label("M-keys:");
            let hot = egui::Color32::from_rgb(127, 224, 160);
            let dim = egui::Color32::from_gray(140);
            for (m, label) in [(MKey::M1, "M1"), (MKey::M2, "M2"), (MKey::M3, "M3"), (MKey::MR, "MR")] {
                let on = snapshot.mkeys.contains(&m);
                ui.colored_label(if on { hot } else { dim }, label);
            }
        });
```

Add `use crate::protocol::MKey;` to the monitor imports (alongside the existing `use crate::protocol::{G13Event, G13Key};` — extend it to include `MKey`).

- [ ] **Step 4: Real Profiles tab**

Replace the `render_profiles` placeholder body in `src/monitor/mod.rs` with:

```rust
    fn render_profiles(&self, ui: &mut egui::Ui) {
        ui.heading("Profiles");
        ui.label("M1/M2/M3 select the bound profile. Click a slot to switch (same as pressing the M-key).");
        ui.add_space(8.0);

        let (active, slots) = {
            let set = self.profiles.read().unwrap();
            let slots = [
                (MKey::M1, set.name(MKey::M1).map(String::from)),
                (MKey::M2, set.name(MKey::M2).map(String::from)),
                (MKey::M3, set.name(MKey::M3).map(String::from)),
            ];
            (set.active(), slots)
        };

        let mut switch_to: Option<MKey> = None;
        for (m, name) in &slots {
            let label = match name {
                Some(n) => format!("{m:?}  —  {n}"),
                None => format!("{m:?}  —  (unassigned)"),
            };
            let is_active = *m == active;
            if ui.add_enabled(name.is_some(), egui::SelectableLabel::new(is_active, label)).clicked() {
                switch_to = Some(*m);
            }
        }
        if let Some(m) = switch_to {
            self.profiles.write().unwrap().set_active(m);
        }

        ui.add_space(10.0);
        ui.separator();
        ui.label("Available in profiles/:");
        for f in self.profiles.read().unwrap().available() {
            ui.weak(f);
        }
        ui.add_space(6.0);
        ui.weak("(assigning files to slots and editing bindings are planned)");
    }
```

- [ ] **Step 5: Build + test**

Run: `cargo build 2>&1 | tail -2` → `Finished`.
Run: `cargo test 2>&1 | tail -3` → full suite green (adds the DeviceState M-key test).

- [ ] **Step 6: Commit**

```bash
git add src/device_state.rs src/monitor/mod.rs
git commit -m "feat: GUI shows active profile, M-key indicator, and profile switching

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 6: Ship manifest + profile files; hardware test; milestone

**Files:** Modify `config.toml`; create `profiles/default.toml`, `profiles/game.toml`, `profiles/media.toml`; modify `milestones/open/v0.2-joystick-mkeys-service.md`.

- [ ] **Step 1: Create the profile files**

Move the current bindings into `profiles/default.toml`. Create `profiles/default.toml` with the current `config.toml` contents (the `[keys]` + `[joystick]` sections). Create `profiles/game.toml` and `profiles/media.toml` as example variants:

`profiles/default.toml` — copy the existing `[keys]` and `[joystick]` from the current `config.toml`.

`profiles/game.toml`:
```toml
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

`profiles/media.toml`:
```toml
[keys]
G1 = "space"
G2 = "left"
G3 = "right"
G4 = "up"
G5 = "down"
```

- [ ] **Step 2: Replace config.toml with the manifest**

Replace `config.toml` contents with:
```toml
# Manifest: which profile file backs each M-key. Files live in profiles_dir.
# Each profile file is a full binding set ([keys] + [joystick]).
profiles_dir = "profiles"
m1 = "default.toml"
m2 = "game.toml"
m3 = "media.toml"
```

- [ ] **Step 3: Build release + full test**

Run: `cargo build --release 2>&1 | tail -2` → `Finished`.
Run: `cargo test 2>&1 | tail -3` → green.

- [ ] **Step 4: Hardware acceptance smoke test (manual — requires the G13 on WinUSB)**

```bash
export PATH="$HOME/.cargo/bin:/c/Strawberry/c/bin:$PATH"
export RUST_LOG=info
./target/release/g13-driver.exe
```
Confirm:
- Header shows `Profile: default.toml`; Profiles tab shows M1=default, M2=game, M3=media with M1 active.
- Press **M2** → header/Profiles active switches to game; the Monitor M-key indicator lights M2; with Active mode + Notepad, G1 now types `1` (game) instead of copying (default).
- Press **M3** → media; **M1** → back to default. Clicking a slot in the Profiles tab switches identically.
- Hold the stick (a joystick key held), then press an M-key → no stuck key (release-on-switch).
- Edit `profiles/game.toml` live → `config reloaded`; switch to M2 shows the change.
- `--headless` still runs.

- [ ] **Step 5: Update the milestone**

In `milestones/open/v0.2-joystick-mkeys-service.md`, tick the M-key decode + profile-switching tasks and add a note that sub-project 2 is complete (Windows Service remains). Commit config + profiles + milestone:

```bash
git add config.toml profiles/ milestones/open/v0.2-joystick-mkeys-service.md
git commit -m "feat: ship profiles manifest + example profiles; mark M-key sub-project done

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Self-Review

**Spec coverage:**
- M-key decode (bytes 6/7, MR ignored heartbeat/click) → Task 1 ✓
- Config split (Profile / Manifest / ProfileSet, manifest + legacy, M1 required, M2/M3 optional, available) → Tasks 2–3 ✓
- Dispatch from active profile; M-keys switch; release-held on switch; MR no-op → Task 4 ✓
- Runtime loads ProfileSet, watches profiles_dir, active-preserving reload → Task 4 ✓
- DeviceState M-keys; header profile; M-key indicator; real Profiles tab (switch + available list) → Task 5 ✓
- Manifest + example profiles shipped; legacy still works; hardware test; milestone → Task 6 ✓
- Testing plan (protocol/config/device_state/dispatcher unit; GUI manual) → Tasks 1,3,4,5 ✓

**Deviations from spec:** none of substance. Implementation detail: the `Config`→`Profile` rename is staged behind a temporary `pub type Config = Profile;` alias (Task 2) removed in Task 4, to keep each task compiling — cleaner than a single crate-wide rename+rewire.

**Placeholder scan:** none — every step has concrete code/commands.

**Type consistency:** `MKey::{M1,M2,M3,MR}`, `G13Event::{MKeyDown,MKeyUp}`, `Profile::{load,get_binding,joystick}`, `ProfileSet::{load,active,active_profile,set_active,name,active_name,available,profiles_dir}`, `Dispatcher::new(Arc<RwLock<ProfileSet>>, _)`, `DeviceState.mkeys` — consistent across tasks.
