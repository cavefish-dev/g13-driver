# Background app (tray / auto-start / single-instance) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the G13 driver a background app — tray icon with a 3-state status, hide-to-tray on close/minimize (driver keeps running), opt-in auto-start at login, single-instance, resumed Active/Dry-run mode, and no console flash.

**Architecture:** The window is decoupled from the process — the tray owns process lifetime; the USB reader + `consumer_loop` threads are untouched (Active injection continues while hidden). Close/minimize are intercepted in eframe `update()` and hide instead of exit. Tray interactions are handled on the UI thread via `tray-icon`'s event handler so they work while hidden.

**Tech Stack:** Rust (GNU toolchain), eframe/egui 0.31, `tray-icon` (+ `muda` menu), `winreg`, `toml_edit`, `windows-sys` (FFI: console attach, mutex/event).

Full design: `docs/superpowers/specs/2026-07-10-background-app-design.md`.

## Global Constraints

- Build with the **GNU** toolchain; if `cargo`/`gcc` are missing from PATH, prepend: `export PATH="$HOME/.cargo/bin:/c/Strawberry/c/bin:$PATH"`. Do **not** switch to the MSVC target.
- **Binary** crate: run `cargo test` (NOT `cargo test --lib`).
- **TDD** for pure-logic (`config.rs` `[app]`, `autostart::run_command`, `tray::icon_state`, icon buffer size). The OS-integration surface — `tray-icon` widgets, `winreg` registry side effects, the named mutex/event, `windows_subsystem`/`AttachConsole`, and the eframe close/minimize/visibility wiring — is the documented **manual-verify** exception (like USB/`SendInput`): no unit tests, verified by the smoke test.
- **No `panic!`/`unwrap()` in the runtime path.** Every OS touchpoint (tray create, registry, mutex/event, `AttachConsole`) **logs a warning and continues**; the app degrades gracefully (no tray → plain window; autostart write fails → toggle reports it, app still runs).
- **Platform isolation:** the new OS modules (`tray.rs`, `autostart.rs`, `single_instance.rs`) are `#[cfg(windows)]`; no Win32 types leak into `dispatcher`/`config`/`protocol`.
- **Pinned deps** (Windows-only unless noted): `tray-icon = "0.24"`, `winreg = "0.56"`, `toml_edit = "0.22"` (matches the copy `toml` 0.8 already pulls in — do not bump). `windows-sys` gains features `Win32_System_Console`, `Win32_System_Threading`, `Win32_Foundation`.
- **Defaults:** auto-start OFF by default; first-run mode = Dry-run (`start_active = false`). `--minimized` starts hidden; no flag shows the window.
- **API-drift note:** for the `tray-icon` and `windows-sys` FFI steps, the target code below is the intended shape; before finalizing, verify the exact signatures against docs.rs for the pinned version and adjust if the API differs. Do NOT change the pinned version to make code compile — adapt the code.
- One focused commit per task; imperative subject; end each commit message with:
  `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`

---

### Task 1: Config — `[app] start_active` (read + format-preserving persist)

**Files:**
- Modify: `src/config.rs` (add `RawApp`, extend `RawManifest`, add `config_path`/`start_active` to `ProfileSet`, accessor, and `persist_start_active` via `toml_edit`; tests in `profileset_tests`)
- Modify: `Cargo.toml` (add `toml_edit = "0.22"`)

**Interfaces:**
- Produces: `ProfileSet::start_active(&self) -> bool` (default false); `ProfileSet::persist_start_active(&self, value: bool) -> anyhow::Result<()>` (writes `[app] start_active` into the manifest, preserving all other keys and comments).
- Consumes: existing `ProfileSet::load`.

- [ ] **Step 1: Add the dependency**

In `Cargo.toml` under `[dependencies]` add:
```toml
toml_edit = "0.22"
```

- [ ] **Step 2: Write the failing tests**

Add to the `profileset_tests` module in `src/config.rs` (it has `write`, `tmp`, `use super::*;`):

```rust
    #[test]
    fn start_active_defaults_false_when_absent() {
        let d = tmp("app-default");
        write(&d, "config.toml", "[keys]\nG1 = \"a\"\n");
        let set = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert!(!set.start_active());
    }

    #[test]
    fn start_active_parses_true() {
        let d = tmp("app-true");
        write(&d.join("profiles"), "default.toml", "[keys]\nG1 = \"a\"\n");
        write(&d, "config.toml",
            "profiles_dir = \"profiles\"\nm1 = \"default.toml\"\n[app]\nstart_active = true\n");
        let set = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert!(set.start_active());
    }

    #[test]
    fn persist_start_active_preserves_other_keys_and_reloads() {
        let d = tmp("app-persist");
        write(&d.join("profiles"), "default.toml", "[keys]\nG1 = \"a\"\n");
        write(&d, "config.toml",
            "# my manifest\nprofiles_dir = \"profiles\"\nm1 = \"default.toml\"\nm2 = \"game.toml\"\n");
        let set = ProfileSet::load(&d.join("config.toml")).unwrap();
        set.persist_start_active(true).unwrap();

        // Reloads as true; other keys + the comment survive.
        let reloaded = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert!(reloaded.start_active());
        let text = std::fs::read_to_string(d.join("config.toml")).unwrap();
        assert!(text.contains("# my manifest"));
        assert!(text.contains("m2 = \"game.toml\""));
        assert!(text.contains("profiles_dir = \"profiles\""));
    }
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test start_active`
Expected: FAIL — `no method named 'start_active'`.

- [ ] **Step 4: Implement**

Add `use std::path::Path;` is already present. Add the `RawApp` struct near `RawManifest` (around line 115):

```rust
#[derive(Debug, Deserialize)]
struct RawApp {
    #[serde(default)]
    start_active: bool,
}
```

Extend `RawManifest`:
```rust
#[derive(Debug, Deserialize)]
struct RawManifest {
    profiles_dir: Option<String>,
    m1: Option<String>,
    m2: Option<String>,
    m3: Option<String>,
    #[serde(default)]
    autorepeat: Option<RawAutoRepeat>,
    #[serde(default)]
    app: Option<RawApp>,
}
```

Add fields to `ProfileSet` (after `autorepeat: AutoRepeat,`):
```rust
    autorepeat: AutoRepeat,
    config_path: PathBuf,
    start_active: bool,
```

In `ProfileSet::load`, right after `let autorepeat = ...;`, add:
```rust
        let start_active = raw.app.as_ref().map(|a| a.start_active).unwrap_or(false);
```
Then add `config_path: config_path.to_path_buf(),` and `start_active,` to **both** `Ok(Self { ... })` constructors.

Add the accessor and persistence near `autorepeat()`:
```rust
    pub fn start_active(&self) -> bool { self.start_active }

    /// Write `[app] start_active` into the manifest, preserving every other key and
    /// comment (format-preserving via toml_edit). Best-effort; callers log on error.
    pub fn persist_start_active(&self, value: bool) -> Result<()> {
        use toml_edit::{DocumentMut, Item, Table, value as toml_value};
        let text = std::fs::read_to_string(&self.config_path)
            .with_context(|| format!("failed to read {}", self.config_path.display()))?;
        let mut doc = text.parse::<DocumentMut>()
            .with_context(|| format!("failed to parse {}", self.config_path.display()))?;
        if !doc.as_table().contains_key("app") {
            doc.as_table_mut().insert("app", Item::Table(Table::new()));
        }
        doc["app"]["start_active"] = toml_value(value);
        std::fs::write(&self.config_path, doc.to_string())
            .with_context(|| format!("failed to write {}", self.config_path.display()))?;
        Ok(())
    }
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test start_active` then `cargo test`
Expected: PASS (3 new) and all existing tests still pass.

- [ ] **Step 6: Commit**

```bash
git add src/config.rs Cargo.toml Cargo.lock
git commit -m "feat: read + persist [app] start_active in the manifest"
```

---

### Task 2: `autostart.rs` — Run-key registration

**Files:**
- Create: `src/autostart.rs`
- Modify: `src/main.rs` (add `mod autostart;`)
- Modify: `Cargo.toml` (add `winreg` under the Windows target deps)

**Interfaces:**
- Produces: `autostart::run_command(exe: &str) -> String`; `autostart::is_enabled() -> bool`; `autostart::enable() -> anyhow::Result<()>`; `autostart::disable() -> anyhow::Result<()>`.

- [ ] **Step 1: Add the dependency**

In `Cargo.toml`, under `[target.'cfg(windows)'.dependencies]` add:
```toml
winreg = "0.56"
```

- [ ] **Step 2: Write the failing test** (pure logic only — registry calls are manual-verify)

Create `src/autostart.rs` with the test first:
```rust
#[cfg(test)]
mod tests {
    use super::run_command;

    #[test]
    fn run_command_quotes_exe_and_adds_minimized_flag() {
        assert_eq!(run_command(r"C:\Program Files\g13\g13-driver.exe"),
                   "\"C:\\Program Files\\g13\\g13-driver.exe\" --minimized");
    }
}
```

- [ ] **Step 3: Run it to verify it fails**

Run: `cargo test run_command`
Expected: FAIL — `cannot find function run_command` (module not yet declared / function missing).

- [ ] **Step 4: Implement**

Prepend to `src/autostart.rs` (above the test module):
```rust
//! Opt-in launch-at-login via the per-user registry Run key.
#![cfg(windows)]

use anyhow::Result;
use winreg::enums::*;
use winreg::RegKey;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "g13-driver";

/// The Run-key command: the quoted exe path plus the start-hidden flag.
pub fn run_command(exe: &str) -> String {
    format!("\"{exe}\" --minimized")
}

/// True if the Run-key value exists.
pub fn is_enabled() -> bool {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(RUN_KEY)
        .and_then(|k| k.get_value::<String, _>(VALUE_NAME))
        .is_ok()
}

/// Register the current exe to launch (minimized) at login.
pub fn enable() -> Result<()> {
    let exe = std::env::current_exe()?;
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(RUN_KEY)?;
    key.set_value(VALUE_NAME, &run_command(&exe.to_string_lossy()))?;
    Ok(())
}

/// Remove the Run-key value (no-op if already absent).
pub fn disable() -> Result<()> {
    let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE)?;
    match key.delete_value(VALUE_NAME) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}
```

Add `mod autostart;` to `src/main.rs` (near the other `mod` lines).

Verify the `winreg` 0.56 API names (`open_subkey`, `open_subkey_with_flags`, `create_subkey`, `get_value`, `set_value`, `delete_value`, `KEY_SET_VALUE`, `HKEY_CURRENT_USER`) against docs.rs; adjust if they differ.

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test run_command` then `cargo build`
Expected: test PASS; clean build (module may warn "unused" until wired in Task 5 — acceptable).

- [ ] **Step 6: Commit**

```bash
git add src/autostart.rs src/main.rs Cargo.toml Cargo.lock
git commit -m "feat: autostart module (HKCU Run key, --minimized)"
```

---

### Task 3: `tray.rs` — status icon + tray + menu

**Files:**
- Create: `src/tray.rs`
- Modify: `src/main.rs` (add `mod tray;`)
- Modify: `Cargo.toml` (add `tray-icon` under the Windows target deps)

**Interfaces:**
- Produces: `tray::IconState` (`Problem`/`Active`/`DryRun`); `tray::icon_state(connected: bool, active: bool) -> IconState`; `tray::icon_rgba(state: IconState) -> (Vec<u8>, u32, u32)`; a `tray::TrayHandle` that owns the `TrayIcon` and exposes `menu_ids()` and `set_state(state)` / `set_checks(active, autostart)`. The exact `TrayHandle` shape is finalized here and consumed in Task 5.

- [ ] **Step 1: Add the dependency**

In `Cargo.toml`, under `[target.'cfg(windows)'.dependencies]` add:
```toml
tray-icon = "0.24"
```

- [ ] **Step 2: Write the failing tests** (pure logic: state precedence + buffer size)

Create `src/tray.rs` with tests first:
```rust
#[cfg(test)]
mod tests {
    use super::{icon_state, icon_rgba, IconState};

    #[test]
    fn problem_takes_precedence_over_mode() {
        assert_eq!(icon_state(false, true), IconState::Problem);   // disconnected beats Active
        assert_eq!(icon_state(false, false), IconState::Problem);
    }

    #[test]
    fn connected_reflects_mode() {
        assert_eq!(icon_state(true, true), IconState::Active);
        assert_eq!(icon_state(true, false), IconState::DryRun);
    }

    #[test]
    fn icon_rgba_is_32x32() {
        let (buf, w, h) = icon_rgba(IconState::Active);
        assert_eq!((w, h), (32, 32));
        assert_eq!(buf.len(), 32 * 32 * 4);
    }
}
```

- [ ] **Step 3: Run to verify fail**

Run: `cargo test icon_state`
Expected: FAIL — module/functions not found.

- [ ] **Step 4: Implement**

Prepend to `src/tray.rs`:
```rust
//! System-tray icon + menu. Windows-only.
#![cfg(windows)]

use anyhow::Result;
use tray_icon::menu::{CheckMenuItem, Menu, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

/// Effective tray state; problem outranks mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconState { Problem, Active, DryRun }

pub fn icon_state(connected: bool, active: bool) -> IconState {
    if !connected { IconState::Problem }
    else if active { IconState::Active }
    else { IconState::DryRun }
}

/// A flat 32x32 RGBA icon in the state's colour (red / green / grey).
pub fn icon_rgba(state: IconState) -> (Vec<u8>, u32, u32) {
    let (r, g, b) = match state {
        IconState::Problem => (210, 70, 70),
        IconState::Active  => (95, 200, 130),
        IconState::DryRun  => (140, 140, 140),
    };
    let mut buf = Vec::with_capacity(32 * 32 * 4);
    for _ in 0..(32 * 32) { buf.extend_from_slice(&[r, g, b, 255]); }
    (buf, 32, 32)
}

fn make_icon(state: IconState) -> Result<Icon> {
    let (rgba, w, h) = icon_rgba(state);
    Ok(Icon::from_rgba(rgba, w, h)?)
}

/// Menu item IDs shared with the consumer (Task 5 matches on these).
pub struct MenuIds {
    pub show: MenuId,
    pub active: MenuId,
    pub autostart: MenuId,
    pub quit: MenuId,
}

/// Owns the tray icon + menu items so they outlive the process.
pub struct TrayHandle {
    icon: TrayIcon,
    item_active: CheckMenuItem,
    item_autostart: CheckMenuItem,
    ids: MenuIds,
    state: IconState,
}

impl TrayHandle {
    /// Build the tray with the initial state + check states.
    pub fn new(state: IconState, active: bool, autostart: bool) -> Result<Self> {
        let item_show = MenuItem::new("Show / Hide window", true, None);
        let item_active = CheckMenuItem::new("Active", true, active, None);
        let item_autostart = CheckMenuItem::new("Start at login", true, autostart, None);
        let item_quit = MenuItem::new("Quit", true, None);

        let menu = Menu::new();
        menu.append(&item_show)?;
        menu.append(&item_active)?;
        menu.append(&item_autostart)?;
        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&item_quit)?;

        let ids = MenuIds {
            show: item_show.id().clone(),
            active: item_active.id().clone(),
            autostart: item_autostart.id().clone(),
            quit: item_quit.id().clone(),
        };

        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip(tooltip(state))
            .with_icon(make_icon(state)?)
            .build()?;

        Ok(Self { icon, item_active, item_autostart, ids, state })
    }

    pub fn ids(&self) -> &MenuIds { &self.ids }

    /// Swap the icon + tooltip only when the effective state changes.
    pub fn set_state(&mut self, state: IconState) {
        if state == self.state { return; }
        self.state = state;
        if let Ok(icon) = make_icon(state) {
            let _ = self.icon.set_icon(Some(icon));
        }
        let _ = self.icon.set_tooltip(Some(tooltip(state)));
    }

    /// Keep the menu checkmarks in sync with the true state.
    pub fn set_checks(&self, active: bool, autostart: bool) {
        self.item_active.set_checked(active);
        self.item_autostart.set_checked(autostart);
    }
}

fn tooltip(state: IconState) -> &'static str {
    match state {
        IconState::Problem => "G13 — not connected",
        IconState::Active  => "G13 — Active",
        IconState::DryRun  => "G13 — Dry-run",
    }
}
```

Add `mod tray;` to `src/main.rs`.

Verify the `tray-icon` 0.24 / `muda` 0.19 API (`TrayIconBuilder`, `Icon::from_rgba`, `Menu::append`, `MenuItem::new`/`CheckMenuItem::new`, `.id()`, `set_icon`, `set_tooltip`, `set_checked`) against docs.rs; adjust names/signatures if they differ (e.g. `with_tooltip` may take `String`).

- [ ] **Step 5: Run tests + build**

Run: `cargo test icon_state` then `cargo build`
Expected: 3 tests PASS; clean build (unused-until-Task-5 warnings acceptable).

- [ ] **Step 6: Commit**

```bash
git add src/tray.rs src/main.rs Cargo.toml Cargo.lock
git commit -m "feat: tray module — 3-state status icon + menu"
```

---

### Task 4: `main.rs` + `single_instance.rs` — no-console, flag, single instance

**Files:**
- Create: `src/single_instance.rs`
- Modify: `src/main.rs` (crate attribute, flag parse, AttachConsole, single-instance guard, pass `start_minimized` to `monitor::run`)
- Modify: `src/monitor/mod.rs` (`run` gains a `start_minimized: bool` param → initial viewport visibility)

**Interfaces:**
- Produces: `single_instance::acquire() -> Acquired` where `Acquired::First(Guard)` means we're the only instance (holds the mutex + an activation-event waiter that shows the window when a later launch pings it) and `Acquired::Already` means another instance is running (the caller shows it and exits). `single_instance::signal_existing()` sets the activation event.
- Consumes: `tray`/`autostart` unaffected; `monitor::run(config, start_minimized)`.

This task is **manual-verify** (OS FFI + process/console wiring — no unit tests).

- [ ] **Step 1: Add windows-sys features**

In `Cargo.toml`, extend the windows-sys features:
```toml
[target.'cfg(windows)'.dependencies]
windows-sys = { version = "0.59", features = [
    "Win32_UI_Input_KeyboardAndMouse",
    "Win32_System_Console",
    "Win32_System_Threading",
    "Win32_Foundation",
] }
```

- [ ] **Step 2: Implement `single_instance.rs`**

Create `src/single_instance.rs`:
```rust
//! Single-instance guard: a named mutex detects a running instance; a named event
//! lets a second launch ask the first to show its window. Windows-only.
#![cfg(windows)]

use std::sync::mpsc::Sender;
use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows_sys::Win32::System::Threading::{
    CreateEventW, CreateMutexW, OpenEventW, SetEvent, WaitForSingleObject, INFINITE,
};

/// A UTF-16, NUL-terminated copy of `s` for the `*W` Win32 APIs.
fn wide_vec(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub enum Acquired {
    /// We are the only instance. Hold this until exit.
    First(Guard),
    /// Another instance is already running.
    Already,
}

pub struct Guard {
    mutex: HANDLE,
}

impl Drop for Guard {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.mutex); }
    }
}

/// Try to become the single instance.
pub fn acquire() -> Acquired {
    let name = wide_vec("Local\\g13-driver-singleton");
    let mutex = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    if mutex.is_null() {
        // Can't create the mutex — fail open (allow launch) rather than block the app.
        return Acquired::First(Guard { mutex: std::ptr::null_mut() });
    }
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe { CloseHandle(mutex); }
        return Acquired::Already;
    }
    Acquired::First(Guard { mutex })
}

/// Ask the running instance to show its window.
pub fn signal_existing() {
    let name = wide_vec("Local\\g13-driver-activate");
    // 0x0002 = EVENT_MODIFY_STATE
    let ev = unsafe { OpenEventW(0x0002, 0, name.as_ptr()) };
    if !ev.is_null() {
        unsafe { SetEvent(ev); CloseHandle(ev); }
    }
}

/// Spawn a waiter that fires `on_activate` whenever a later launch pings the event.
/// Call once, from the first instance, after the window exists.
pub fn spawn_activation_waiter(on_activate: Sender<()>) {
    let name = wide_vec("Local\\g13-driver-activate");
    let ev = unsafe { CreateEventW(std::ptr::null(), 0, 0, name.as_ptr()) };
    if ev.is_null() { return; }
    std::thread::spawn(move || loop {
        let r = unsafe { WaitForSingleObject(ev, INFINITE) };
        if r != 0 { break; } // WAIT_OBJECT_0 == 0; anything else -> stop
        if on_activate.send(()).is_err() { break; }
    });
}
```

Remove the broken `wide!` macro and its two `const … = &wide!(…)` lines — they were a placeholder; the code uses `wide_vec()` at runtime instead. Verify every `windows-sys` 0.59 symbol used (`CreateMutexW`, `CreateEventW`, `OpenEventW`, `SetEvent`, `WaitForSingleObject`, `INFINITE`, `ERROR_ALREADY_EXISTS`, `GetLastError`, `CloseHandle`, `HANDLE`) and its signature (pointer vs `PCWSTR`, `BOOL` as `i32`) against docs.rs; adjust casts as needed.

- [ ] **Step 3: Wire `main.rs`**

Set the crate attribute at the very top of `src/main.rs` (above the existing `#[cfg(not(windows))] compile_error!` line is fine; the attribute must be an inner attribute at crate root):
```rust
#![cfg_attr(windows, windows_subsystem = "windows")]
```

Add module declarations: `mod single_instance;` (already added `mod autostart;`/`mod tray;` in Tasks 2/3).

Replace `fn main()` body so it: attaches a console for `--headless`, enforces single instance for the GUI, parses `--minimized`, and passes it through:
```rust
fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let headless = args.iter().any(|a| a == "--headless");
    let minimized = args.iter().any(|a| a == "--minimized");

    #[cfg(windows)]
    if headless {
        // Reattach to the launching terminal so logs are visible under windows_subsystem=windows.
        unsafe { windows_sys::Win32::System::Console::AttachConsole(u32::MAX); } // ATTACH_PARENT_PROCESS
    }

    env_logger::init();

    let config = runtime::load_config_and_watch(PathBuf::from("config.toml"))?;

    if headless {
        let rx = runtime::spawn_usb_reader()?;
        return runtime::run_headless(config, rx);
    }

    // GUI: enforce single instance.
    #[cfg(windows)]
    {
        match single_instance::acquire() {
            single_instance::Acquired::Already => {
                single_instance::signal_existing();
                return Ok(());
            }
            single_instance::Acquired::First(guard) => {
                // Keep the guard alive for the whole GUI session.
                let _guard = guard;
                return monitor::run(config, minimized);
            }
        }
    }
    #[cfg(not(windows))]
    monitor::run(config, minimized)
}
```
(Verify `AttachConsole` takes `u32`; `ATTACH_PARENT_PROCESS` is `0xFFFF_FFFF` = `u32::MAX`. If windows-sys exports the constant, use it instead of the literal.)

- [ ] **Step 4: Add the `start_minimized` param to `monitor::run`**

In `src/monitor/mod.rs`, change `pub fn run(config: Arc<RwLock<ProfileSet>>) -> Result<()>` to
`pub fn run(config: Arc<RwLock<ProfileSet>>, start_minimized: bool) -> Result<()>` and set the
initial viewport visibility:
```rust
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([660.0, 560.0])
            .with_resizable(false)
            .with_visible(!start_minimized),
```

- [ ] **Step 5: Build + existing suite**

Run: `cargo build && cargo test`
Expected: clean build (pre-existing `usb.rs` warning + any unused-until-Task-5 tray warnings), all tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/main.rs src/single_instance.rs src/monitor/mod.rs Cargo.toml Cargo.lock
git commit -m "feat: no-console GUI, --minimized, single-instance guard"
```

---

### Task 5: Monitor integration — tray lifecycle, hide-to-tray, settings

**Files:**
- Modify: `src/monitor/mod.rs`

**Interfaces:**
- Consumes: `tray::{TrayHandle, icon_state}` (Task 3), `autostart` (Task 2), `ProfileSet::start_active`/`persist_start_active` (Task 1), `single_instance::spawn_activation_waiter` (Task 4).

This task is **manual-verify** (eframe/tray wiring). Build must stay green; behaviour is checked in Task 6.

- [ ] **Step 1: Initialize mode from config and build the tray**

In `run`, set the initial mode from the manifest and (Windows) build the tray + activation waiter. Change the `dry_run` init:
```rust
    let start_active = config.read().unwrap().start_active();
    let dry_run = Arc::new(AtomicBool::new(!start_active));
```
Add fields to `MonitorApp`:
```rust
    #[cfg(windows)]
    tray: Option<crate::tray::TrayHandle>,
    #[cfg(windows)]
    activate_rx: Option<std::sync::mpsc::Receiver<()>>,
    last_persisted_active: bool,
    last_icon: Option<crate::tray::IconState>,
```
In `MonitorApp::new`, build the tray from the current state and spawn the activation waiter (Windows only), storing `activate_rx`; initialize `last_persisted_active` to `!dry_run` and `last_icon` to `None`. Wrap tray creation so a failure logs and leaves `tray = None` (plain-window fallback):
```rust
        #[cfg(windows)]
        let (tray, activate_rx) = {
            let active = !app.dry_run.load(Ordering::Relaxed);
            let st = crate::tray::icon_state(false, active); // not connected yet at startup
            let tray = crate::tray::TrayHandle::new(st, active, crate::autostart::is_enabled())
                .map_err(|e| log::warn!("tray unavailable: {e:#}")).ok();
            let (tx, rx) = std::sync::mpsc::channel();
            crate::single_instance::spawn_activation_waiter(tx);
            (tray, Some(rx))
        };
```
(Thread these into the struct initializer. Non-Windows builds omit the tray fields.)

- [ ] **Step 2: Hide-to-tray + activation + icon sync + tray events in `update()`**

At the top of `eframe::App::update`, before rendering, add (Windows-gated where it touches the tray):

```rust
        // Close (X) / Minimize -> hide to tray instead of exiting.
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        if ctx.input(|i| i.viewport().minimized == Some(true)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }

        // A second launch asked us to show the window.
        #[cfg(windows)]
        if let Some(rx) = &self.activate_rx {
            if rx.try_recv().is_ok() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
        }

        // Drain tray menu events.
        #[cfg(windows)]
        if let Some(tray) = &mut self.tray {
            while let Ok(ev) = tray_icon::menu::MenuEvent::receiver().try_recv() {
                let ids = tray.ids();
                if ev.id == ids.show {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                } else if ev.id == ids.active {
                    let now_active = self.dry_run.load(Ordering::Relaxed); // was dry-run -> go active
                    self.dry_run.store(!now_active, Ordering::Relaxed);
                } else if ev.id == ids.autostart {
                    let r = if crate::autostart::is_enabled() { crate::autostart::disable() }
                            else { crate::autostart::enable() };
                    if let Err(e) = r { log::warn!("autostart toggle failed: {e:#}"); }
                } else if ev.id == ids.quit {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            // Left-click / double-click the icon -> toggle window visibility.
            while let Ok(_ev) = tray_icon::TrayIconEvent::receiver().try_recv() {
                // Any click event toggles; refine to Click/DoubleClick if desired after verifying the enum.
                let visible = ctx.input(|i| i.viewport().focused).unwrap_or(false);
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(!visible));
            }
        }

        let snapshot = self.state.lock().unwrap().clone();

        // Persist a mode change (from any surface: header, Settings, tray).
        let active = !self.dry_run.load(Ordering::Relaxed);
        if active != self.last_persisted_active {
            self.last_persisted_active = active;
            if let Err(e) = self.profiles.read().unwrap().persist_start_active(active) {
                log::warn!("could not persist mode: {e:#}");
            }
        }

        // Sync the tray icon + checkmarks.
        #[cfg(windows)]
        if let Some(tray) = &mut self.tray {
            let connected = matches!(snapshot.connection, Connection::Connected);
            let st = crate::tray::icon_state(connected, active);
            if self.last_icon != Some(st) { tray.set_state(st); self.last_icon = Some(st); }
            tray.set_checks(active, crate::autostart::is_enabled());
        }
```
Note: the existing `let snapshot = self.state.lock().unwrap().clone();` at the current top of `update` must be removed (it is moved into the block above) so `snapshot` is defined once.

Verify the `tray-icon` 0.24 event API: `MenuEvent::receiver()`, `TrayIconEvent::receiver()`, `MenuEvent.id`, and the `ViewportInfo` fields (`close_requested()`, `minimized: Option<bool>`, `focused: Option<bool>`) against docs.rs; adjust field/method names if they differ. If the visibility of the window can't be read to implement icon-toggle cleanly, track a `visible: bool` on `MonitorApp` instead.

- [ ] **Step 3: Real "Start at login" in the Settings tab**

In `render_settings`, replace the mockup "Launch at login" checkbox with a live one and keep the Dry-run toggle (which already drives `dry_run`). Drop the now-meaningless "Start minimized to tray" mockup line:
```rust
    fn render_settings(&self, ui: &mut egui::Ui) {
        ui.heading("Settings");
        ui.add_space(8.0);
        let mut dry = self.dry_run.load(Ordering::Relaxed);
        if ui.checkbox(&mut dry, "Start in Dry-run (safe)").changed() {
            self.dry_run.store(dry, Ordering::Relaxed);
        }
        #[cfg(windows)]
        {
            let mut on = crate::autostart::is_enabled();
            if ui.checkbox(&mut on, "Launch at login").changed() {
                let r = if on { crate::autostart::enable() } else { crate::autostart::disable() };
                if let Err(e) = r {
                    log::warn!("autostart toggle failed: {e:#}");
                    ui.colored_label(egui::Color32::from_rgb(220, 90, 90), format!("failed: {e:#}"));
                }
            }
        }
        ui.add_space(6.0);
        ui.weak("Close or minimize hides to the tray; the driver keeps running. Quit from the tray to exit.");
    }
```
(`render_settings` takes `&self`; the `dry_run`/`autostart` calls need no `&mut self`. If the borrow checker objects to `self.dry_run.store` under `&self`, it won't — `Arc<AtomicBool>` is shared and store takes `&self`.)

- [ ] **Step 4: Build + suite**

Run: `cargo build && cargo test`
Expected: clean build (only the pre-existing `usb.rs` warning), all tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/monitor/mod.rs
git commit -m "feat: tray lifecycle, hide-to-tray, and live Start-at-login"
```

---

### Task 6: Smoke test, example config, milestone

**Files:**
- Modify: `config.toml` (document `[app]`)
- Create: `milestones/finished/background-app.md`

- [ ] **Step 1: Document `[app]` in `config.toml`**

Append to `config.toml`:
```toml

# App state (managed by the GUI; safe to edit while stopped).
# [app]
# start_active = false   # resume Active (true) or Dry-run (false) on next launch
```

- [ ] **Step 2: Build release and smoke test**

Run:
```bash
cargo build --release
./target/release/g13-driver.exe
```

Guide the user (needs the G13):
1. Window shows. **Close (X)** → window hides, a tray icon appears; press bound keys → they still inject (driver kept running).
2. **Left-click the tray icon** → window reappears; click again (or Close) → hides.
3. Right-click tray → **Active** toggles mode; the tray icon changes **green↔grey** and the tooltip updates.
4. **Unplug** the G13 → tray icon goes **red** (`not connected`); replug + open window + **Retry** → back to green/grey.
5. Settings (or tray) **Start at login** → check `HKCU\...\CurrentVersion\Run` gains a `g13-driver` value; untick → it's removed.
6. With it running, launch the exe again → the existing window is focused, no second process (Task Manager shows one).
7. Toggle to **Active**, **Quit** from the tray, relaunch → it comes back **Active** (mode persisted). First-ever run is Dry-run.
8. Run `./target/release/g13-driver.exe --headless` from the terminal → logs still print. Run `--minimized` → starts hidden in the tray.

- [ ] **Step 3: Write the milestone**

Create `milestones/finished/background-app.md`:
```markdown
# Background app: tray, auto-start, single-instance

- **Status:** finished
- **Date:** <fill in on completion>

## Outcome
The driver runs as a background app. Spec: `docs/superpowers/specs/2026-07-10-background-app-design.md`;
plan: `docs/superpowers/plans/2026-07-10-background-app.md`.
- Window decoupled from process: Close/Minimize hide to tray (driver keeps injecting); Quit from the
  tray is the only exit. Tray interactions are handled on the UI thread so they work while hidden.
- 3-state status icon (red problem > green Active > grey Dry-run), generated in code.
- Opt-in auto-start via the HKCU Run key (`--minimized`); single instance via a named mutex +
  activation event; last Active/Dry-run mode persisted in `config.toml` `[app] start_active`.
- No console flash (`windows_subsystem = "windows"`); `--headless` reattaches the parent console.
- New deps (Windows-only): tray-icon, winreg, toml_edit.

## Follow-ups
- Auto-reconnect while hidden (today: red icon -> open -> Retry).
- Toast/balloon notifications; profile switching from the tray.
- This is MVP sub-project #1 of 3 — next: (#2) GitHub Actions CI driven by version.txt; (#3) auto-update.
```

- [ ] **Step 4: Commit**

```bash
git add config.toml milestones/finished/background-app.md
git commit -m "docs: background-app example config and milestone"
```

---

## Notes for the executor

- After all tasks: run the final whole-branch review (most capable model), then use `superpowers:finishing-a-development-branch`.
- Several tasks leave temporary "unused" warnings between tasks (tray/autostart used only in Task 5); that's expected — the final build is warning-clean.
- The `tray-icon`, `winreg`, and `windows-sys` FFI code is the intended shape; the implementer must confirm exact signatures against docs.rs for the pinned versions and adapt the code (never the pinned version) if they differ. These are manual-verify tasks — the smoke test is the real gate.
- If a GUI smoke test rewrites `profiles/default.toml`, restore it with `git checkout -- profiles/default.toml` (known comment-preservation follow-up).
