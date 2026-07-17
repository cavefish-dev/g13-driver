# Configurable LCD — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the LCD's three lines configurable via a global `[lcd]` section, add a stateful activity tracker (discrete keys + joystick directions, held-vs-last), config-driven rendering, and a clock — with LCD-tab GUI controls.

**Architecture:** `LcdConfig` (enums + bools) parses from `[lcd]` (mirrors `[backlight]`). A stateful `ActivityTracker` replaces the stateless `capture`, feeding line 3. `render(model, cfg)` interprets the config; the poller builds an enriched `LcdModel` (filename, display name, clock string, chosen line-3 action) and both runtimes feed the tracker. GUI controls on the LCD tab persist to `[lcd]`.

**Tech Stack:** Rust, egui, `windows-sys` (already a dep, for the clock). No new dependencies.

## Global Constraints
- **GNU toolchain only.** Build/test with `stable-x86_64-pc-windows-gnu`; MinGW gcc at `C:\Strawberry\c\bin\gcc.exe`. If `cargo`/`gcc` not found, prepend to PATH per CLAUDE.md. Do NOT switch to the MSVC target.
- **TDD** for pure logic (`LcdConfig` enums, `[lcd]` parse/persist, `ActivityTracker`, `render`). GUI + the `GetLocalTime` clock helper are no-unit-test (manual verify), like existing USB/GUI code. `render` stays **pure** — the clock string is passed in via `LcdModel`.
- **Error policy:** no `panic!`/`unwrap()` on the runtime path beyond the accepted lock idiom. A bad `[lcd]` enum value → `log::warn!` + that field's default (never fail load), mirroring `[backlight]`.
- **Config defaults** (missing `[lcd]` or field): `line1_left="name"`, `line1_clock=false`, `line1_mode="label"`, `line2_source="filename"`, `line3_trigger="last"`, `line3_mapping=true`, `line3_label=true`.
- **Font is ASCII** (`0x20..=0x7E`); line-2 text is sanitized (non-ASCII → `*`).
- One focused commit per task; imperative subject line.

## File Structure
- **Modify** `src/lcd/mod.rs` — `LcdConfig` + enums; `ActivityTracker`; enriched `LcdModel`; `render(model, cfg)`; `local_hh_mm`; `sanitize`; `spawn_poller` signature.
- **Modify** `src/config.rs` — `RawLcd`; `ProfileSet.lcd` field; parse; `lcd_config()`; setters; `persist_lcd()`.
- **Modify** `src/runtime.rs`, `src/monitor/mod.rs` — swap the last-action cell for the tracker; feed `on_event`; poller/preview read `current(trigger)`; LCD-tab GUI controls.
- **Modify** `config.toml`, milestone.

---

## Task 1: `LcdConfig` + enums (pure)

**Files:** Modify `src/lcd/mod.rs`; Test `src/lcd/mod.rs`.

**Interfaces (produces):**
- `pub enum Line1Left { Name, Version }`, `pub enum ModeDisplay { Label, Icon, Off }`, `pub enum Line2Source { Filename, Display }`, `pub enum Line3Trigger { Last, Held }` — each `Clone, Copy, PartialEq, Eq, Debug` with `pub fn parse(&str) -> Option<Self>` and `pub fn as_str(&self) -> &'static str`.
- `pub struct LcdConfig { line1_left, line1_clock: bool, line1_mode, line2_source, line3_trigger, line3_mapping: bool, line3_label: bool }` (`Clone, Copy, PartialEq, Debug`) + `Default`.

- [ ] **Step 1: Write failing tests** (in `src/lcd/mod.rs` tests):

```rust
#[test]
fn enum_parse_and_as_str_round_trip() {
    assert_eq!(Line1Left::parse("version"), Some(Line1Left::Version));
    assert_eq!(Line1Left::Version.as_str(), "version");
    assert_eq!(ModeDisplay::parse("off"), Some(ModeDisplay::Off));
    assert_eq!(Line2Source::parse("display"), Some(Line2Source::Display));
    assert_eq!(Line3Trigger::parse("held"), Some(Line3Trigger::Held));
    assert_eq!(Line1Left::parse("bogus"), None);
}

#[test]
fn lcd_config_default() {
    let d = LcdConfig::default();
    assert_eq!(d.line1_left, Line1Left::Name);
    assert!(!d.line1_clock);
    assert_eq!(d.line1_mode, ModeDisplay::Label);
    assert_eq!(d.line2_source, Line2Source::Filename);
    assert_eq!(d.line3_trigger, Line3Trigger::Last);
    assert!(d.line3_mapping && d.line3_label);
}
```

- [ ] **Step 2: Run → fail.** `cargo test lcd::` → FAIL (no `Line1Left`).

- [ ] **Step 3: Implement** (in `src/lcd/mod.rs`):

```rust
macro_rules! str_enum {
    ($name:ident { $($variant:ident => $s:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        pub enum $name { $($variant),+ }
        impl $name {
            pub fn parse(s: &str) -> Option<Self> {
                match s.trim().to_ascii_lowercase().as_str() {
                    $($s => Some(Self::$variant),)+
                    _ => None,
                }
            }
            pub fn as_str(&self) -> &'static str {
                match self { $(Self::$variant => $s),+ }
            }
        }
    };
}
str_enum!(Line1Left { Name => "name", Version => "version" });
str_enum!(ModeDisplay { Label => "label", Icon => "icon", Off => "off" });
str_enum!(Line2Source { Filename => "filename", Display => "display" });
str_enum!(Line3Trigger { Last => "last", Held => "held" });

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct LcdConfig {
    pub line1_left: Line1Left,
    pub line1_clock: bool,
    pub line1_mode: ModeDisplay,
    pub line2_source: Line2Source,
    pub line3_trigger: Line3Trigger,
    pub line3_mapping: bool,
    pub line3_label: bool,
}
impl Default for LcdConfig {
    fn default() -> Self {
        Self {
            line1_left: Line1Left::Name,
            line1_clock: false,
            line1_mode: ModeDisplay::Label,
            line2_source: Line2Source::Filename,
            line3_trigger: Line3Trigger::Last,
            line3_mapping: true,
            line3_label: true,
        }
    }
}
```

- [ ] **Step 4: Run → pass.** `cargo test lcd::`
- [ ] **Step 5: Commit** `git commit -m "feat(lcd): LcdConfig with parseable per-line enums"`

---

## Task 2: `[lcd]` parse into `ProfileSet` (config.rs)

**Files:** Modify `src/config.rs`; Test `src/config.rs`.

Mirror the `[backlight]` pattern exactly (see `RawBacklight` ~line 293, the `let backlight = raw.backlight.map(...)` parse ~line 438, the `backlight` field on `ProfileSet` ~line 421 set in BOTH constructors, and `backlight_config()` ~line 531).

**Interfaces:** `ProfileSet::lcd_config(&self) -> crate::lcd::LcdConfig`. New private field `lcd: crate::lcd::LcdConfig`.

- [ ] **Step 1: Write failing tests** (in `config::tests`):

```rust
#[test]
fn lcd_section_parses() {
    let raw: RawManifest = toml::from_str(
        "[lcd]\nline1_left = \"version\"\nline1_clock = true\nline1_mode = \"off\"\n\
         line2_source = \"display\"\nline3_trigger = \"held\"\nline3_mapping = false\n").unwrap();
    // (load via a temp manifest — follow the sibling backlight test's temp-dir pattern)
}
```

Write it concretely like the existing `parses_backlight_section` test: create a temp manifest `config.toml` with a `[lcd]` block + a profile, `ProfileSet::load`, then assert `set.lcd_config()` fields. Add a `missing_lcd_section_uses_defaults` test (`lcd_config() == LcdConfig::default()`) and a `bad_lcd_enum_falls_back` test (`line1_mode = "bogus"` → `ModeDisplay::Label`, load still Ok).

- [ ] **Step 2: Run → fail.**

- [ ] **Step 3: Implement:**

Add `RawLcd` (near `RawBacklight`), all `Option` with `#[serde(default)]`:

```rust
#[derive(Debug, Deserialize)]
struct RawLcd {
    #[serde(default)] line1_left: Option<String>,
    #[serde(default)] line1_clock: Option<bool>,
    #[serde(default)] line1_mode: Option<String>,
    #[serde(default)] line2_source: Option<String>,
    #[serde(default)] line3_trigger: Option<String>,
    #[serde(default)] line3_mapping: Option<bool>,
    #[serde(default)] line3_label: Option<bool>,
}
```

Add `#[serde(default)] lcd: Option<RawLcd>,` to `RawManifest` (near `backlight`). Add `lcd: crate::lcd::LcdConfig,` to `struct ProfileSet` (near `backlight`).

Parse in `load()` (near the `backlight` parse), warn-and-default per field:

```rust
        let lcd = raw.lcd.map(|l| {
            use crate::lcd::{LcdConfig, Line1Left, ModeDisplay, Line2Source, Line3Trigger};
            let d = LcdConfig::default();
            let enum_parse = |s: Option<String>, parse: fn(&str) -> Option<_>, dflt, field: &str| {
                match s {
                    Some(v) => parse(&v).unwrap_or_else(|| { log::warn!("invalid [lcd] {field} {v:?}; using default"); dflt }),
                    None => dflt,
                }
            };
            LcdConfig {
                line1_left: enum_parse(l.line1_left, Line1Left::parse, d.line1_left, "line1_left"),
                line1_clock: l.line1_clock.unwrap_or(d.line1_clock),
                line1_mode: enum_parse(l.line1_mode, ModeDisplay::parse, d.line1_mode, "line1_mode"),
                line2_source: enum_parse(l.line2_source, Line2Source::parse, d.line2_source, "line2_source"),
                line3_trigger: enum_parse(l.line3_trigger, Line3Trigger::parse, d.line3_trigger, "line3_trigger"),
                line3_mapping: l.line3_mapping.unwrap_or(d.line3_mapping),
                line3_label: l.line3_label.unwrap_or(d.line3_label),
            }
        }).unwrap_or_default();
```

(If the closure's generic `fn(&str)->Option<_>` gives inference trouble, split into four explicit `enum_parse::<Line1Left>`-style calls or inline each field — whatever compiles; the behavior is: present+unparseable → warn + default, absent → default.)

Set `lcd,` in BOTH `Self { ... }` constructors in `load()`. Add the getter:

```rust
    pub fn lcd_config(&self) -> crate::lcd::LcdConfig { self.lcd }
```

- [ ] **Step 4: Run → pass + build.** `cargo test config::` then `cargo build`.
- [ ] **Step 5: Commit** `git commit -m "feat(config): parse [lcd] section into ProfileSet"`

---

## Task 3: `[lcd]` setters + `persist_lcd` (config.rs)

**Files:** Modify `src/config.rs`; Test `src/config.rs`.

Mirror `persist_backlight` (~line 517). **Interfaces:** setters `set_lcd_line1_left`, `set_lcd_line1_clock`, `set_lcd_line1_mode`, `set_lcd_line2_source`, `set_lcd_line3_trigger`, `set_lcd_line3_mapping`, `set_lcd_line3_label` (each updates `self.lcd`); `persist_lcd(&self) -> Result<()>`.

- [ ] **Step 1: Write failing test** — mutate several fields, `persist_lcd()`, reload, assert survived + `[lcd]` in the text (mirror `persist_backlight_round_trips`).

- [ ] **Step 2: Run → fail.**

- [ ] **Step 3: Implement** setters (each `self.lcd.<field> = v;`) and:

```rust
    pub fn persist_lcd(&self) -> Result<()> {
        use toml_edit::{DocumentMut, Item, Table, value as toml_value};
        let text = std::fs::read_to_string(&self.config_path)
            .with_context(|| format!("failed to read {}", self.config_path.display()))?;
        let mut doc = text.parse::<DocumentMut>()
            .with_context(|| format!("failed to parse {}", self.config_path.display()))?;
        if !doc.as_table().contains_key("lcd") {
            doc.as_table_mut().insert("lcd", Item::Table(Table::new()));
        }
        let c = &self.lcd;
        doc["lcd"]["line1_left"] = toml_value(c.line1_left.as_str());
        doc["lcd"]["line1_clock"] = toml_value(c.line1_clock);
        doc["lcd"]["line1_mode"] = toml_value(c.line1_mode.as_str());
        doc["lcd"]["line2_source"] = toml_value(c.line2_source.as_str());
        doc["lcd"]["line3_trigger"] = toml_value(c.line3_trigger.as_str());
        doc["lcd"]["line3_mapping"] = toml_value(c.line3_mapping);
        doc["lcd"]["line3_label"] = toml_value(c.line3_label);
        std::fs::write(&self.config_path, doc.to_string())
            .with_context(|| format!("failed to write {}", self.config_path.display()))?;
        Ok(())
    }
```

- [ ] **Step 4: Run → pass.**
- [ ] **Step 5: Commit** `git commit -m "feat(config): [lcd] setters + format-preserving persist"`

---

## Task 4: `ActivityTracker` (pure, TDD)

**Files:** Modify `src/lcd/mod.rs`; Test `src/lcd/mod.rs`.

**Interfaces:** `pub struct ActivityTracker` with `pub fn new() -> Self`, `pub fn on_event(&mut self, event: &G13Event, profiles: &Arc<RwLock<ProfileSet>>)`, `pub fn current(&self, trigger: Line3Trigger) -> Option<LastAction>`. Consumes `JoystickMapper`, `HoldAction`, `JoystickDir`, `Profile` accessors.

- [ ] **Step 1: Write failing tests** (in `src/lcd/mod.rs` tests; reuse a temp-profile fixture like the `capture` tests). Cover:
  - `KeyDown(G1)` then `current(Last)` → `{button:"G1", combo:Some("ctrl+c"), label:Some("Copy")}`; `current(Held)` same; after `KeyUp(G1)` → `current(Held)` is `None` but `current(Last)` still `Some`.
  - Joystick: build a profile with `[joystick] up="w"` + `[joystick.labels] up="Fwd"`; `on_event(JoystickMove{x:127,y:0})` → `current(Held)` = `{button:"Up", combo:Some("w"), label:Some("Fwd")}`; `JoystickMove{x:127,y:127}` (center) → `current(Held)` None.
  - `MKeyDown(M2)` clears held (after a held key, `current(Held)` None) but `current(Last)` persists.

```rust
#[test]
fn tracker_key_held_and_last() {
    let p = profiles_fixture(); // [keys] G1="ctrl+c" [labels] G1="Copy"
    let mut t = ActivityTracker::new();
    t.on_event(&G13Event::KeyDown(G13Key::G1), &p);
    let want = LastAction { button: "G1".into(), combo: Some("ctrl+c".into()), label: Some("Copy".into()) };
    assert_eq!(t.current(Line3Trigger::Held), Some(want.clone()));
    assert_eq!(t.current(Line3Trigger::Last), Some(want.clone()));
    t.on_event(&G13Event::KeyUp(G13Key::G1), &p);
    assert_eq!(t.current(Line3Trigger::Held), None);
    assert_eq!(t.current(Line3Trigger::Last), Some(want));
}
```

(Write the joystick + MKey tests analogously with their own fixtures.)

- [ ] **Step 2: Run → fail.**

- [ ] **Step 3: Implement:**

```rust
use crate::joystick::{HoldAction, JoystickMapper};
use crate::config::JoystickDir;

#[derive(Clone, Copy, PartialEq, Eq)]
enum HeldId { Key(G13Key), Dir(JoystickDir) }

pub struct ActivityTracker {
    mapper: JoystickMapper,
    held: Vec<(HeldId, LastAction)>, // ordered, most-recent last
    last: Option<LastAction>,
}

impl ActivityTracker {
    pub fn new() -> Self { Self { mapper: JoystickMapper::new(), held: Vec::new(), last: None } }

    fn upsert(&mut self, id: HeldId, action: LastAction) {
        self.held.retain(|(hid, _)| *hid != id);
        self.held.push((id, action.clone()));
        self.last = Some(action);
    }
    fn remove(&mut self, id: HeldId) {
        self.held.retain(|(hid, _)| *hid != id);
    }

    pub fn on_event(&mut self, event: &G13Event, profiles: &Arc<RwLock<ProfileSet>>) {
        match event {
            G13Event::KeyDown(key) => {
                let key = *key;
                let (combo, label) = {
                    let set = profiles.read().unwrap();
                    match set.active_profile() {
                        Some(p) => (p.get_binding(key).map(str::to_string), p.label(key).map(str::to_string)),
                        None => (None, None),
                    }
                };
                self.upsert(HeldId::Key(key), LastAction { button: format!("{key:?}"), combo, label });
            }
            G13Event::KeyUp(key) => self.remove(HeldId::Key(*key)),
            G13Event::JoystickMove { x, y } => {
                let (cfg, deadzone) = {
                    let set = profiles.read().unwrap();
                    (set.active_profile().and_then(|p| p.joystick()).cloned(), set.joystick_deadzone())
                };
                let Some(jc) = cfg else { return };
                let actions = self.mapper.update(*x, *y, &jc, deadzone);
                for a in actions {
                    match a {
                        HoldAction::KeyDown { dir, key } => {
                            let label = {
                                let set = profiles.read().unwrap();
                                set.active_profile().and_then(|p| p.joystick_label(dir)).map(str::to_string)
                            };
                            self.upsert(HeldId::Dir(dir), LastAction {
                                button: format!("{dir:?}"), combo: Some(key), label,
                            });
                        }
                        HoldAction::KeyUp { dir, .. } => self.remove(HeldId::Dir(dir)),
                    }
                }
            }
            G13Event::MKeyDown(_) => { self.held.clear(); self.mapper = JoystickMapper::new(); }
            G13Event::MKeyUp(_) => {}
        }
    }

    pub fn current(&self, trigger: Line3Trigger) -> Option<LastAction> {
        match trigger {
            Line3Trigger::Last => self.last.clone(),
            Line3Trigger::Held => self.held.last().map(|(_, a)| a.clone()),
        }
    }
}
```

- [ ] **Step 4: Run → pass + full `cargo test`.**
- [ ] **Step 5: Commit** `git commit -m "feat(lcd): stateful ActivityTracker for keys + joystick directions"`

---

## Task 5: Config-driven `render` + clock + enriched `LcdModel` + poller

**Files:** Modify `src/lcd/mod.rs` (render, LcdModel, local_hh_mm, sanitize, spawn_poller); Modify `src/monitor/mod.rs` (`render_lcd` build the new model); Test `src/lcd/mod.rs`.

**Note:** `render` is pure (clock passed in). `spawn_poller`/`render_lcd` still read the existing `last_action` cell here — Task 6 swaps in the tracker. Changing `LcdModel` + `render`'s signature breaks the existing render tests and both call sites; update them all so the build stays green.

- [ ] **Step 1: Update/add render tests** to the new shape (each builds an `LcdConfig` + enriched `LcdModel`):

```rust
fn model(last: Option<LastAction>) -> LcdModel {
    LcdModel { mode: Mode::Active, slot: MKey::M2,
        filename: Some("basic".into()), display_name: Some("My Set".into()),
        last, clock: None }
}

#[test]
fn render_line1_version_and_clock() {
    let mut cfg = LcdConfig::default();
    cfg.line1_left = Line1Left::Version;
    cfg.line1_clock = true;
    let mut m = model(None); m.clock = Some("12:34".into());
    let fb = render(&m, &cfg); // must not panic; title band has content
    assert!((0..LCD_W).any(|x| (0..8).any(|y| fb.get(x, y))));
}

#[test]
fn render_line2_display_and_sanitize() {
    let mut cfg = LcdConfig::default();
    cfg.line2_source = Line2Source::Display;
    let mut m = model(None); m.display_name = Some("Ünïcode".into());
    let fb = render(&m, &cfg); // non-ASCII replaced by '*', no panic
    assert!((0..LCD_W).any(|x| (12..27).any(|y| fb.get(x, y))));
}

#[test]
fn render_line3_flags() {
    let last = Some(LastAction { button: "G1".into(), combo: Some("ctrl+c".into()), label: Some("Copy".into()) });
    let mut cfg = LcdConfig::default();
    cfg.line3_mapping = false; cfg.line3_label = false;
    let fb = render(&model(last), &cfg); // only the button shows; no panic
    assert!((0..LCD_W).any(|x| (30..40).any(|y| fb.get(x, y))));
}

#[test]
fn sanitize_replaces_non_ascii() {
    assert_eq!(sanitize("aé1"), "a*1");
    assert_eq!(sanitize("ok"), "ok");
}
```

- [ ] **Step 2: Run → fail** (LcdModel shape / `render(m,cfg)` / `sanitize`).

- [ ] **Step 3: Implement.** Replace `LcdModel` + `render` and add helpers:

```rust
#[derive(Clone, PartialEq, Debug)]
pub struct LcdModel {
    pub mode: Mode,
    pub slot: MKey,
    pub filename: Option<String>,
    pub display_name: Option<String>,
    pub last: Option<LastAction>,
    pub clock: Option<String>,
}

pub fn sanitize(s: &str) -> String {
    s.chars().map(|c| if (' '..='~').contains(&c) { c } else { '*' }).collect()
}

#[cfg(windows)]
pub fn local_hh_mm() -> String {
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    let mut st: windows_sys::Win32::Foundation::SYSTEMTIME = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut st) };
    format!("{:02}:{:02}", st.wHour, st.wMinute)
}
#[cfg(not(windows))]
pub fn local_hh_mm() -> String { String::new() }

pub fn render(model: &LcdModel, cfg: &LcdConfig) -> Framebuffer {
    let mut fb = Framebuffer::new();

    // Line 1 left.
    let version = format!("v{}", env!("G13_VERSION"));
    let left = match cfg.line1_left { Line1Left::Name => "G13 Driver", Line1Left::Version => &version };
    fb.draw_text(0, 0, left, 1);

    // Line 1 right cluster: [clock] [mode]. Build right-to-left.
    let mode_text = match model.mode { Mode::Active => "ACTIVE", Mode::DryRun => "DRY-RUN" };
    let filled = matches!(model.mode, Mode::Active);
    let mut x = LCD_W as i32;
    if cfg.line1_mode == ModeDisplay::Label {
        x -= text_width(mode_text, 1);
        fb.draw_text(x, 0, mode_text, 1);
        x -= 8; // box + gap
    } else if cfg.line1_mode == ModeDisplay::Icon {
        x -= 6;
    }
    if cfg.line1_mode != ModeDisplay::Off {
        let bx = x; // draw the box at bx..bx+6
        if filled { fb.fill_rect(bx, 1, 6, 6, true); }
        else {
            fb.draw_hline(bx, 1, 6); fb.draw_hline(bx, 6, 6);
            for dy in 1..7 { fb.set_pixel(bx, dy, true); fb.set_pixel(bx + 5, dy, true); }
        }
        x -= 3; // gap before clock
    }
    if cfg.line1_clock {
        if let Some(clk) = &model.clock {
            x -= text_width(clk, 1);
            fb.draw_text(x, 0, clk, 1);
        }
    }

    // Divider.
    fb.draw_hline(0, 9, LCD_W as i32);

    // Line 2: slot + name (2x), per source, sanitized.
    fb.draw_text(0, 16, slot_label(model.slot), 1);
    let raw_name = match cfg.line2_source {
        Line2Source::Filename => model.filename.clone(),
        Line2Source::Display => model.display_name.clone().or_else(|| model.filename.clone()),
    };
    let name = match raw_name { Some(n) => truncate(&sanitize(&n), 12), None => "(empty)".to_string() };
    fb.draw_text(18, 12, &name, 2);

    // Line 3: button [+ combo] [+ label] per flags.
    if let Some(a) = &model.last {
        let mut line = a.button.clone();
        if cfg.line3_mapping {
            match &a.combo {
                Some(c) => { line.push_str("  "); line.push_str(c); }
                None => line.push_str("  (unbound)"),
            }
        }
        if cfg.line3_label {
            if let Some(l) = &a.label { line.push_str("  "); line.push_str(l); }
        }
        fb.draw_text(0, 32, &truncate(&line, 26), 1);
    }
    fb
}
```

Update `spawn_poller` (still using the `last` cell) to build the enriched model + config + clock:

```rust
pub fn spawn_poller(
    profiles: Arc<RwLock<ProfileSet>>,
    dry_run: Arc<AtomicBool>,
    last: Arc<Mutex<Option<LastAction>>>,
    frame: Arc<Mutex<[u8; 992]>>,
) {
    thread::spawn(move || loop {
        let (cfg, model) = {
            let set = profiles.read().unwrap();
            let cfg = set.lcd_config();
            let clock = if cfg.line1_clock { Some(local_hh_mm()) } else { None };
            let model = LcdModel {
                mode: if dry_run.load(Ordering::Relaxed) { Mode::DryRun } else { Mode::Active },
                slot: set.active(),
                filename: set.active_name_stem().map(str::to_string),
                display_name: set.active_profile().and_then(|p| p.meta_name()).map(str::to_string),
                last: last.lock().unwrap().clone(),
                clock,
            };
            (cfg, model)
        };
        *frame.lock().unwrap() = render(&model, &cfg).pack();
        thread::sleep(Duration::from_millis(150));
    });
}
```

Update `render_lcd` in `src/monitor/mod.rs` (the `LcdModel { ... }` at ~line 1338) to the new fields + `render(&model, &self.profiles.read().unwrap().lcd_config())`. It still reads `self.last_action.lock().unwrap().clone()` for `last` here (Task 6 swaps to the tracker). Build the model under short read locks, matching the existing guard scoping.

- [ ] **Step 4: Run → pass + build + full `cargo test`.**
- [ ] **Step 5: Commit** `git commit -m "feat(lcd): config-driven render with clock, sanitize, enriched model"`

---

## Task 6: Swap the last-action cell for the `ActivityTracker`

**Files:** Modify `src/runtime.rs`, `src/monitor/mod.rs`, `src/lcd/mod.rs` (poller signature).

Replace `Arc<Mutex<Option<LastAction>>>` + `capture(...)` with `Arc<Mutex<ActivityTracker>>` + `tracker.on_event(...)`; the poller and GUI preview read `current(cfg.line3_trigger)`. Remove the now-unused `capture` function.

- [ ] **Step 1: `spawn_poller` takes the tracker.** Change its `last: Arc<Mutex<Option<LastAction>>>` param to `tracker: Arc<Mutex<ActivityTracker>>`, and the model's `last:` to:

```rust
                last: tracker.lock().unwrap().current(cfg.line3_trigger),
```

- [ ] **Step 2: headless (`src/runtime.rs`).** Replace `let last_action = Arc::new(Mutex::new(None));` with `let tracker = Arc::new(Mutex::new(crate::lcd::ActivityTracker::new()));`; pass `tracker.clone()` to `spawn_poller`; in the dispatch loop replace `crate::lcd::capture(&event, &config, &last_action);` with `tracker.lock().unwrap().on_event(&event, &config);`.

- [ ] **Step 3: GUI (`src/monitor/mod.rs`).** Change the `MonitorApp.last_action` field type + init to `Arc<Mutex<crate::lcd::ActivityTracker>>` / `ActivityTracker::new()`; rename to `tracker` for clarity. In `start_consumer`, pass `self.tracker.clone()` to `spawn_poller` and thread it into `consumer_loop`; in `consumer_loop` replace `capture(...)` with `tracker.lock().unwrap().on_event(&event, &profiles)`. In `render_lcd`, replace the `last:` field with `self.tracker.lock().unwrap().current(cfg.line3_trigger)` (read `cfg = self.profiles.read().unwrap().lcd_config()` first, in its own statement, to avoid holding a guard across the tracker lock).

- [ ] **Step 4: Remove `capture`.** Delete the now-unused `pub fn capture` and its tests in `src/lcd/mod.rs` (the tracker tests supersede them).

- [ ] **Step 5: Build + full test.** `cargo build` then `cargo test` — clean; the LCD now shows joystick directions on line 3 and honors `line3_trigger`.
- [ ] **Step 6: Commit** `git commit -m "feat(lcd): drive line 3 from ActivityTracker in both runtimes"`

---

## Task 7: LCD-tab GUI controls

**Files:** Modify `src/monitor/mod.rs` (`render_lcd`).

Above the preview, add controls persisting to `[lcd]` (mirror the Settings backlight block's setter + `persist_lcd()` pattern, ~line 1438). GUI — manual verify, no unit test.

- [ ] **Step 1: Add the controls.** In `render_lcd`, before the preview, read `let cfg = self.profiles.read().unwrap().lcd_config();` (own statement), then render widgets; on change, call the matching setter + `persist_lcd()` (log warn on error). Use `egui::ComboBox` for the four enums and `ui.checkbox` for the three bools. Example for one dropdown + one checkbox (do all seven):

```rust
        let cfg = self.profiles.read().unwrap().lcd_config();
        let mut changed = false;
        egui::ComboBox::from_label("Line 1 left")
            .selected_text(cfg.line1_left.as_str())
            .show_ui(ui, |ui| {
                for opt in [crate::lcd::Line1Left::Name, crate::lcd::Line1Left::Version] {
                    let mut sel = cfg.line1_left == opt;
                    if ui.selectable_label(sel, opt.as_str()).clicked() {
                        self.profiles.write().unwrap().set_lcd_line1_left(opt);
                        changed = true;
                    }
                    let _ = &mut sel;
                }
            });
        let mut clock = cfg.line1_clock;
        if ui.checkbox(&mut clock, "Clock").changed() {
            self.profiles.write().unwrap().set_lcd_line1_clock(clock);
            changed = true;
        }
        // ... repeat for line1_mode (Label/Icon/Off), line2_source (Filename/Display),
        //     line3_trigger (Last/Held), line3_mapping, line3_label ...
        if changed {
            if let Err(e) = self.profiles.read().unwrap().persist_lcd() {
                log::warn!("persist lcd failed: {e:#}");
            }
        }
```

**Borrow rule:** never hold a `self.profiles.read()` guard across a `self.profiles.write()` — each `read()`/`write()` is its own statement (the `cfg` snapshot is `Copy`, dropped immediately).

- [ ] **Step 2: Build.** `cargo build` clean.
- [ ] **Step 3: Manual check.** `cargo run` → LCD tab: each control changes the preview live and writes `[lcd]` to `config.toml`.
- [ ] **Step 4: Commit** `git commit -m "feat(gui): LCD-tab controls for the [lcd] config"`

---

## Task 8: Config example, milestone, smoke

**Files:** Modify `config.toml`; Move milestone.

- [ ] **Step 1:** Append a commented `[lcd]` example to `config.toml` (the seven keys with their default values + a short comment).
- [ ] **Step 2:** Edit `milestones/open/configurable-lcd.md` → `Status: ongoing`, check boxes, add a smoke checklist (each knob changes LCD+preview; joystick labels on line 3 in both triggers; clock ticks; unicode→`*`). `git mv` to `milestones/ongoing/`.
- [ ] **Step 3:** `cargo test && cargo build --release` — all pass, clean.
- [ ] **Step 4: Commit** `git commit -m "docs: configurable-lcd milestone to ongoing + config example"`

---

## Self-Review

**Spec coverage:** `[lcd]` config (Tasks 1-3, 8); ActivityTracker held-vs-last + joystick (Task 4); config-driven render + clock + sanitize (Task 5); wiring both runtimes + drop `capture` (Task 6); GUI controls (Task 7). ✓

**Placeholder scan:** Task 2's test is described-then-write-concretely (mirror `parses_backlight_section`) — the backlight sibling is the exact template; behavior fully specified. All other steps have complete code.

**Build-green ordering:** Task 4 adds the tracker alongside `capture` (still used); Task 5 changes `render`/`LcdModel` and updates BOTH call sites (poller + preview) using the existing cell; Task 6 swaps cell→tracker and removes `capture`. Each task compiles.

**Type consistency:** `LcdConfig`/enums (`parse`/`as_str`), `lcd_config()`, `set_lcd_*`, `persist_lcd`, `ActivityTracker::{new,on_event,current}`, `Line3Trigger`, enriched `LcdModel { mode, slot, filename, display_name, last, clock }`, `render(model, cfg)`, `local_hh_mm`, `sanitize`, `spawn_poller(profiles, dry_run, tracker, frame)` consistent across tasks.
