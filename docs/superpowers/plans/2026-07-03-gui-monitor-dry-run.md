# GUI Monitor (Dry-run Test Tool) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A default-launch `egui`/`eframe` window that shows the G13's live input (G-keys + joystick) and its configured mapping, with a Dry-run/Active toggle that gates keystroke injection — so the driver can be exercised without firing events into other Windows apps.

**Architecture:** GUI on the main thread (winit requirement). A background consumer thread drains the `G13Event` stream, updates a shared `DeviceState` for display, and — only when Active — forwards events to the existing `Dispatcher` to inject. First launch is Dry-run. Shared startup wiring is lifted from `main` into a `runtime` module reused by both the GUI and a preserved `--headless` console mode.

**Tech Stack:** Rust, GNU toolchain (`stable-x86_64-pc-windows-gnu`), `eframe`/`egui` (pure Rust, builds on GNU), `rusb`, `windows-sys`, `toml`/`serde`, `log`. Build/test with `cargo` (PATH may need `export PATH="$HOME/.cargo/bin:/c/Strawberry/c/bin:$PATH"`).

## Global Constraints

- **Windows-only** (`src/main.rs:1-2` enforces `compile_error!` off-Windows). OS injection stays behind `#[cfg(windows)]` in `src/injector/`. The GUI (`eframe`) is platform-neutral — no Win32 types in `monitor`/`device_state`/`runtime`.
- **TDD** for pure logic (`DeviceState::apply`). `eframe` rendering, the consumer thread, and USB/`SendInput`/wiring code have NO unit tests (documented manual-verify exception) — verified by the hardware smoke test.
- **First launch = Dry-run** (`AtomicBool` starts `true`). Dry-run = no `SendInput`. Active→Dry-run transition (and disconnect) must call `dispatcher.release_held()` so no key is left stuck.
- **Never crash on device problems** — the window opens even with no G13; connection status is shown; a manual **Retry** re-attempts. No auto-reconnect polling this round.
- **Preserve `--headless`** — the current console driver (always Active), windowless.
- **Module named `runtime`, not `core`** (avoids shadowing the std `core` crate).
- **Error policy:** injection failures `log::warn!` and continue; no `panic!`/`unwrap()` in the runtime/consumer path (test code may `unwrap`).
- **Commits:** one per task; imperative subject; end every commit message with the trailer `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.
- Binary crate — run `cargo test` (not `cargo test --lib`); focused: `cargo test <module>::`.
- **eframe API note:** the code below targets `eframe = "0.31"`. egui's API shifts between 0.x releases. If a different version resolves, adapt small API differences (`run_native` signature, `CreationContext.egui_ctx`, panel/`painter` calls) using current docs (Context7 / docs.rs) — the shapes shown are representative, not sacred. If eframe fails to build on the GNU toolchain, STOP and escalate (do not switch to MSVC).

---

## File Structure

| File | Change | Responsibility |
|------|--------|----------------|
| `src/device_state.rs` | Create | `DeviceState`/`Connection` reducer — pure `apply(&G13Event)` (TDD) |
| `src/runtime.rs` | Create | Shared wiring: `load_config_and_watch`, `spawn_usb_reader`, `run_headless`, `watch_config` (moved from main) |
| `src/monitor/mod.rs` | Create | `MonitorApp` (eframe), consumer thread, `run()`, rendering |
| `src/protocol.rs` | Modify | Add `G13Key::ALL` (ordered list for the grid) |
| `src/main.rs` | Modify | Slim to mode selection: default → `monitor::run`, `--headless` → `runtime::run_headless` |
| `Cargo.toml` | Modify | Add `eframe = "0.31"` |

---

## Task 1: DeviceState reducer

**Files:**
- Create: `src/device_state.rs`
- Modify: `src/main.rs` (add `mod device_state;`)

**Interfaces:**
- Produces:
  - `pub enum Connection { Connected, Disconnected(String) }` (derives `Debug, Clone, PartialEq, Eq`)
  - `pub struct DeviceState { pub pressed: HashSet<G13Key>, pub joy_x: u8, pub joy_y: u8, pub connection: Connection }` (derives `Debug, Clone`)
  - `DeviceState::new() -> Self` / `Default` (empty, joy centered at 127, `Disconnected("connecting")`)
  - `DeviceState::apply(&mut self, event: &G13Event)`

- [ ] **Step 1: Add the module declaration**

In `src/main.rs`, add `mod device_state;` to the module list (alphabetical, after `mod dispatcher;`). (Other modules from later tasks — `runtime`, `monitor` — are added in their own tasks.)

- [ ] **Step 2: Write the failing tests**

Create `src/device_state.rs` with ONLY this test module (implementation comes in Step 4):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{G13Event, G13Key};

    #[test]
    fn default_is_centered_and_empty() {
        let s = DeviceState::new();
        assert!(s.pressed.is_empty());
        assert_eq!(s.joy_x, 127);
        assert_eq!(s.joy_y, 127);
        assert_eq!(s.connection, Connection::Disconnected("connecting".to_string()));
    }

    #[test]
    fn key_down_inserts() {
        let mut s = DeviceState::new();
        s.apply(&G13Event::KeyDown(G13Key::G1));
        assert!(s.pressed.contains(&G13Key::G1));
    }

    #[test]
    fn key_up_removes() {
        let mut s = DeviceState::new();
        s.apply(&G13Event::KeyDown(G13Key::G1));
        s.apply(&G13Event::KeyUp(G13Key::G1));
        assert!(!s.pressed.contains(&G13Key::G1));
    }

    #[test]
    fn key_up_of_unpressed_is_noop() {
        let mut s = DeviceState::new();
        s.apply(&G13Event::KeyUp(G13Key::G5)); // never pressed
        assert!(s.pressed.is_empty());
    }

    #[test]
    fn multiple_keys_tracked() {
        let mut s = DeviceState::new();
        s.apply(&G13Event::KeyDown(G13Key::G1));
        s.apply(&G13Event::KeyDown(G13Key::G2));
        assert_eq!(s.pressed.len(), 2);
        assert!(s.pressed.contains(&G13Key::G1));
        assert!(s.pressed.contains(&G13Key::G2));
    }

    #[test]
    fn joystick_move_updates_axes() {
        let mut s = DeviceState::new();
        s.apply(&G13Event::JoystickMove { x: 10, y: 240 });
        assert_eq!(s.joy_x, 10);
        assert_eq!(s.joy_y, 240);
    }
}
```

- [ ] **Step 3: Run to verify failure**

Run: `cargo test device_state:: 2>&1 | tail -15`
Expected: FAIL — compile error `cannot find struct DeviceState` / `cannot find enum Connection`.

- [ ] **Step 4: Implement the reducer above the test module**

Prepend to `src/device_state.rs`:

```rust
use std::collections::HashSet;
use crate::protocol::{G13Event, G13Key};

/// USB connection status shown in the monitor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Connection {
    Connected,
    Disconnected(String),
}

/// Live snapshot of the G13's input, reconstructed from the G13Event stream.
/// Pure and platform-neutral so it can be unit-tested and rendered by the GUI.
/// Extend here (M-keys, joystick click) when the parser decodes bytes 6/7.
#[derive(Debug, Clone)]
pub struct DeviceState {
    pub pressed: HashSet<G13Key>,
    pub joy_x: u8,
    pub joy_y: u8,
    pub connection: Connection,
}

impl Default for DeviceState {
    fn default() -> Self {
        Self {
            pressed: HashSet::new(),
            joy_x: 127,
            joy_y: 127,
            connection: Connection::Disconnected("connecting".to_string()),
        }
    }
}

impl DeviceState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fold one event into the live state. KeyDown/KeyUp maintain the pressed
    /// set; JoystickMove updates the axes. Connection is set by the consumer,
    /// not by events.
    pub fn apply(&mut self, event: &G13Event) {
        match event {
            G13Event::KeyDown(k) => { self.pressed.insert(*k); }
            G13Event::KeyUp(k) => { self.pressed.remove(k); }
            G13Event::JoystickMove { x, y } => {
                self.joy_x = *x;
                self.joy_y = *y;
            }
        }
    }
}
```

- [ ] **Step 5: Run to verify pass**

Run: `cargo test device_state:: 2>&1 | tail -5`
Expected: PASS — 6 device_state tests green.

- [ ] **Step 6: Commit**

```bash
git add src/device_state.rs src/main.rs
git commit -m "feat: add DeviceState reducer for live input display

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Extract shared wiring into `runtime`

**Files:**
- Create: `src/runtime.rs`
- Modify: `src/main.rs` (add `mod runtime;`; slim `main` to call runtime helpers)

**Interfaces:**
- Consumes: `Config` (config.rs), `UsbReader` (usb.rs), `Dispatcher` (dispatcher.rs), `WindowsInjector` (injector/windows.rs).
- Produces:
  - `runtime::load_config_and_watch(path: PathBuf) -> anyhow::Result<Arc<RwLock<Config>>>`
  - `runtime::spawn_usb_reader() -> anyhow::Result<Receiver<G13Event>>`
  - `runtime::run_headless(config: Arc<RwLock<Config>>, rx: Receiver<G13Event>) -> anyhow::Result<()>`

- [ ] **Step 1: Create `src/runtime.rs` with the extracted wiring**

This moves the current `watch_config`, USB-reader spawn, and console loop out of `main` verbatim (behavior identical). Create `src/runtime.rs`:

```rust
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, RwLock};
use std::thread;
use anyhow::Result;
use crate::config::{Config, JoystickMode};
use crate::protocol::G13Event;
use crate::{dispatcher, injector, usb};

/// Load config and spawn the hot-reload watcher thread. Returns the shared handle.
pub fn load_config_and_watch(path: PathBuf) -> Result<Arc<RwLock<Config>>> {
    let config = Arc::new(RwLock::new(Config::load(&path)?));
    {
        let config = config.clone();
        let path = path.clone();
        thread::spawn(move || watch_config(config, path));
    }
    Ok(config)
}

/// Open the G13 and spawn the USB reader thread. Returns the event channel.
/// Returns Err (does not exit) so callers — e.g. the GUI — can show the error.
pub fn spawn_usb_reader() -> Result<Receiver<G13Event>> {
    let (tx, rx) = mpsc::channel();
    let reader = usb::UsbReader::open()?;
    thread::spawn(move || {
        if let Err(e) = reader.run(tx) {
            log::error!("USB reader stopped: {e:#}");
        }
    });
    Ok(rx)
}

/// The console driver: consume events, inject, release held keys on exit.
pub fn run_headless(config: Arc<RwLock<Config>>, rx: Receiver<G13Event>) -> Result<()> {
    let injector = Box::new(injector::windows::WindowsInjector::new());
    let mut dispatcher = dispatcher::Dispatcher::new(config.clone(), injector);

    if let Some(j) = config.read().unwrap().joystick() {
        if j.mode == JoystickMode::Mouse {
            log::warn!("joystick mouse mode is configured but not yet implemented; stick will be inert");
        }
    }

    log::info!("g13-driver running (headless) — press Ctrl+C to stop");

    for event in rx {
        if let Err(e) = dispatcher.handle(event) {
            log::warn!("dispatch error: {e:#}");
        }
    }

    dispatcher.release_held();
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

- [ ] **Step 2: Slim `src/main.rs` to use `runtime`**

Replace the entire body of `src/main.rs` (keep the `compile_error!` at top) with:

```rust
#[cfg(not(windows))]
compile_error!("g13-driver v0.1 targets Windows only; Linux support is planned for v1.0");

mod config;
mod device_state;
mod dispatcher;
mod injector;
mod joystick;
mod protocol;
mod runtime;
mod usb;

use anyhow::Result;
use std::path::PathBuf;

fn main() -> Result<()> {
    env_logger::init();

    let config = runtime::load_config_and_watch(PathBuf::from("config.toml"))?;
    let rx = runtime::spawn_usb_reader()?;
    runtime::run_headless(config, rx)
}
```

(Note: `mod monitor;` and the GUI default are added in Task 3. For now `main` stays headless so behavior is unchanged and verifiable.)

- [ ] **Step 3: Build and run the full suite — confirm no regressions**

Run: `cargo test 2>&1 | tail -5`
Expected: `test result: ok. 59 passed` (53 prior + 6 from Task 1; no new tests here).

Run: `cargo build 2>&1 | tail -2`
Expected: `Finished` with no errors. (`main.rs` is much smaller; the console driver behaves exactly as before.)

- [ ] **Step 4: Commit**

```bash
git add src/runtime.rs src/main.rs
git commit -m "refactor: extract startup wiring into runtime module

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: eframe skeleton window + mode selection

**Files:**
- Modify: `Cargo.toml` (add `eframe`)
- Create: `src/monitor/mod.rs` (skeleton `MonitorApp` + `run`)
- Modify: `src/main.rs` (add `mod monitor;`; select mode)

**Interfaces:**
- Consumes: `Config` (Arc<RwLock>), `DeviceState`/`Connection` (Task 1).
- Produces: `monitor::run(config: Arc<RwLock<Config>>) -> anyhow::Result<()>`.

- [ ] **Step 1: Add the eframe dependency**

In `Cargo.toml`, under `[dependencies]`, add:

```toml
eframe  = "0.31"
```

- [ ] **Step 2: Verify eframe builds on this toolchain (de-risk)**

Run: `cargo build 2>&1 | tail -5`
Expected: eframe + egui + winit + glow compile; `Finished`. **If it fails to build on the GNU toolchain, STOP and report BLOCKED** (do not switch to the MSVC target).

- [ ] **Step 3: Create the skeleton monitor window**

Create `src/monitor/mod.rs`:

```rust
use std::sync::{Arc, Mutex, RwLock};
use std::sync::atomic::AtomicBool;
use anyhow::Result;
use eframe::egui;
use crate::config::Config;
use crate::device_state::{Connection, DeviceState};

/// Launch the monitor window. Blocks on the eframe event loop (main thread).
pub fn run(config: Arc<RwLock<Config>>) -> Result<()> {
    let state = Arc::new(Mutex::new(DeviceState::new()));
    let dry_run = Arc::new(AtomicBool::new(true)); // first launch = Dry-run

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([720.0, 500.0]),
        ..Default::default()
    };

    eframe::run_native(
        "G13 Monitor",
        options,
        Box::new(move |_cc| Ok(Box::new(MonitorApp { config, state, dry_run }))),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {e}"))?;
    Ok(())
}

pub struct MonitorApp {
    config: Arc<RwLock<Config>>,
    state: Arc<Mutex<DeviceState>>,
    dry_run: Arc<AtomicBool>,
}

impl eframe::App for MonitorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let connection = self.state.lock().unwrap().connection.clone();
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("G13 Monitor");
            match &connection {
                Connection::Connected => ui.label("● Connected"),
                Connection::Disconnected(why) => ui.label(format!("○ Disconnected — {why}")),
            };
            ui.label("(skeleton — live monitor wired up in the next task)");
        });
    }
}
```

(The `config`/`dry_run` fields are unused in this skeleton — Task 4 consumes them. If the compiler warns about unused fields, that is expected and cleared in Task 4; do not add `#[allow]`.)

- [ ] **Step 4: Wire mode selection in `src/main.rs`**

Add `mod monitor;` (after `mod joystick;`) and replace `main` with:

```rust
fn main() -> Result<()> {
    env_logger::init();

    let config = runtime::load_config_and_watch(PathBuf::from("config.toml"))?;

    if std::env::args().any(|a| a == "--headless") {
        let rx = runtime::spawn_usb_reader()?;
        runtime::run_headless(config, rx)
    } else {
        monitor::run(config)
    }
}
```

- [ ] **Step 5: Build + full test suite**

Run: `cargo build 2>&1 | tail -2` → `Finished`.
Run: `cargo test 2>&1 | tail -5` → `59 passed`.

- [ ] **Step 6: Manual check — window opens**

Run: `cargo run` → a "G13 Monitor" window opens showing the heading and a disconnected line. Close it. Run: `cargo run -- --headless` → the console driver runs as before (needs the G13; Ctrl+C to stop). Confirm both.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock src/monitor/mod.rs src/main.rs
git commit -m "feat: add eframe skeleton window and --headless mode selection

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: Consumer thread + live monitor + Dry-run/Active gating

**Files:**
- Modify: `src/protocol.rs` (add `G13Key::ALL`)
- Modify: `src/monitor/mod.rs` (consumer thread, live rendering, toggle)

**Interfaces:**
- Consumes: `runtime::spawn_usb_reader`, `Dispatcher::{new, handle, release_held}`, `WindowsInjector`, `Config::{get_binding, joystick}`, `DeviceState`.
- Produces: `G13Key::ALL: [G13Key; 22]`; a working live monitor with a `Dry-run`/`Active` toggle.

- [ ] **Step 1: Add and test `G13Key::ALL`**

In `src/protocol.rs`, add a failing test to the test module first:

```rust
    #[test]
    fn all_lists_every_key_once() {
        use std::collections::HashSet;
        assert_eq!(G13Key::ALL.len(), 22);
        let unique: HashSet<_> = G13Key::ALL.iter().collect();
        assert_eq!(unique.len(), 22);
        assert_eq!(G13Key::ALL[0], G13Key::G1);
        assert_eq!(G13Key::ALL[21], G13Key::G22);
    }
```

Run: `cargo test protocol::tests::all_lists_every_key_once 2>&1 | tail -8` → FAIL (`no associated item named ALL`).

Then add to `impl` scope in `src/protocol.rs` (place an `impl G13Key { ... }` block after the enum):

```rust
impl G13Key {
    /// All 22 G-keys in ascending order, for iteration (e.g. the monitor grid).
    pub const ALL: [G13Key; 22] = [
        G13Key::G1,  G13Key::G2,  G13Key::G3,  G13Key::G4,  G13Key::G5,
        G13Key::G6,  G13Key::G7,  G13Key::G8,  G13Key::G9,  G13Key::G10,
        G13Key::G11, G13Key::G12, G13Key::G13, G13Key::G14, G13Key::G15,
        G13Key::G16, G13Key::G17, G13Key::G18, G13Key::G19, G13Key::G20,
        G13Key::G21, G13Key::G22,
    ];
}
```

Run the test again → PASS.

- [ ] **Step 2: Add the consumer loop and wire it in `MonitorApp`**

Replace the whole body of `src/monitor/mod.rs` with the version below. It adds: a `consumer_loop` (drains events, updates `DeviceState`, injects only when Active, releases held keys on Active→Dry-run and on disconnect, and requests repaints), a `start_consumer` that attempts the USB open and sets `Connection`, and live rendering of keys + joystick + the toggle. (This is manual-verify code — no unit test; it is exercised by the hardware smoke test in Task 6.)

```rust
use std::sync::{Arc, Mutex, RwLock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;
use anyhow::Result;
use eframe::egui;
use crate::config::Config;
use crate::device_state::{Connection, DeviceState};
use crate::dispatcher::Dispatcher;
use crate::injector::windows::WindowsInjector;
use crate::protocol::{G13Event, G13Key};
use crate::runtime;

pub fn run(config: Arc<RwLock<Config>>) -> Result<()> {
    let state = Arc::new(Mutex::new(DeviceState::new()));
    let dry_run = Arc::new(AtomicBool::new(true)); // first launch = Dry-run

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([720.0, 500.0]),
        ..Default::default()
    };

    eframe::run_native(
        "G13 Monitor",
        options,
        Box::new(move |cc| Ok(Box::new(MonitorApp::new(cc, config, state, dry_run)))),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {e}"))?;
    Ok(())
}

pub struct MonitorApp {
    config: Arc<RwLock<Config>>,
    state: Arc<Mutex<DeviceState>>,
    dry_run: Arc<AtomicBool>,
}

impl MonitorApp {
    fn new(
        cc: &eframe::CreationContext<'_>,
        config: Arc<RwLock<Config>>,
        state: Arc<Mutex<DeviceState>>,
        dry_run: Arc<AtomicBool>,
    ) -> Self {
        let app = Self { config, state, dry_run };
        app.start_consumer(cc.egui_ctx.clone());
        app
    }

    /// Attempt to open the G13 and spawn the consumer thread. Sets connection
    /// status accordingly. Called at startup and on Retry (only while
    /// disconnected, so no second reader races the first).
    fn start_consumer(&self, ctx: egui::Context) {
        match runtime::spawn_usb_reader() {
            Ok(rx) => {
                self.state.lock().unwrap().connection = Connection::Connected;
                let injector = Box::new(WindowsInjector::new());
                let dispatcher = Dispatcher::new(self.config.clone(), injector);
                let state = self.state.clone();
                let dry_run = self.dry_run.clone();
                std::thread::spawn(move || consumer_loop(rx, dispatcher, state, dry_run, ctx));
            }
            Err(e) => {
                self.state.lock().unwrap().connection = Connection::Disconnected(format!("{e:#}"));
            }
        }
    }
}

/// Drains the event stream: updates DeviceState for display always; injects via
/// the dispatcher only when Active. A 50ms recv timeout lets us notice a
/// Dry-run toggle promptly so an Active->Dry-run switch releases held keys even
/// with no new events. Exits when the channel closes (device unplugged).
fn consumer_loop(
    rx: Receiver<G13Event>,
    mut dispatcher: Dispatcher,
    state: Arc<Mutex<DeviceState>>,
    dry_run: Arc<AtomicBool>,
    ctx: egui::Context,
) {
    let mut was_active = !dry_run.load(Ordering::Relaxed);
    loop {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(event) => {
                state.lock().unwrap().apply(&event);
                let active = !dry_run.load(Ordering::Relaxed);
                if was_active && !active {
                    dispatcher.release_held();
                }
                if active {
                    if let Err(e) = dispatcher.handle(event) {
                        log::warn!("dispatch error: {e:#}");
                    }
                }
                was_active = active;
                ctx.request_repaint();
            }
            Err(RecvTimeoutError::Timeout) => {
                let active = !dry_run.load(Ordering::Relaxed);
                if was_active && !active {
                    dispatcher.release_held();
                }
                was_active = active;
            }
            Err(RecvTimeoutError::Disconnected) => {
                dispatcher.release_held();
                state.lock().unwrap().connection =
                    Connection::Disconnected("device disconnected".to_string());
                ctx.request_repaint();
                return;
            }
        }
    }
}

impl eframe::App for MonitorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let snapshot = self.state.lock().unwrap().clone();

        egui::TopBottomPanel::top("hd").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("G13 Monitor");
                match &snapshot.connection {
                    Connection::Connected => ui.colored_label(egui::Color32::from_rgb(95, 214, 138), "● Connected"),
                    Connection::Disconnected(why) => ui.colored_label(egui::Color32::from_rgb(220, 90, 90), format!("○ {why}")),
                };
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut active = !self.dry_run.load(Ordering::Relaxed);
                    if ui.selectable_label(active, "Active").clicked() { active = true; }
                    if ui.selectable_label(!active, "Dry-run").clicked() { active = false; }
                    self.dry_run.store(!active, Ordering::Relaxed);
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let cfg = self.config.read().unwrap();
            ui.horizontal_wrapped(|ui| {
                for key in G13Key::ALL {
                    let pressed = snapshot.pressed.contains(&key);
                    let binding = cfg.get_binding(key).unwrap_or("—");
                    let text = format!("{key:?}\n{binding}");
                    let color = if pressed { egui::Color32::from_rgb(20, 54, 31) } else { egui::Color32::from_gray(38) };
                    egui::Frame::none().fill(color).inner_margin(4.0).show(ui, |ui| {
                        ui.set_width(58.0);
                        ui.label(text);
                    });
                }
            });
            ui.separator();
            ui.label(format!("joystick  x={}  y={}", snapshot.joy_x, snapshot.joy_y));
        });
    }
}
```

- [ ] **Step 3: Build + full test suite**

Run: `cargo build 2>&1 | tail -2` → `Finished` (the Task 3 unused-field warnings are now gone).
Run: `cargo test 2>&1 | tail -5` → `60 passed` (Task 1's 6 + Task 4's `all_lists_every_key_once` + prior 53).

- [ ] **Step 4: Commit**

```bash
git add src/protocol.rs src/monitor/mod.rs
git commit -m "feat: live monitor via consumer thread with Dry-run/Active gating

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: Layout polish + joystick panel + Retry

**Files:**
- Modify: `src/monitor/mod.rs` (physical-layout grid, joystick box, status footer, Retry button)

**Interfaces:**
- Consumes: everything from Task 4. No new public interface.

- [ ] **Step 1: Render the physical layout, joystick box, footer, and Retry**

Replace the `impl eframe::App for MonitorApp { fn update ... }` block in `src/monitor/mod.rs` with the richer rendering below. Key rows mirror the device; the joystick is drawn as a box with a deadzone circle and a live position dot; WASD labels highlight active directions; a footer shows config/joystick settings; a **Retry connection** button appears only while disconnected.

```rust
// Rows approximating the physical G13 key arrangement.
const ROWS: [&[G13Key]; 6] = [
    &[G13Key::G1, G13Key::G2, G13Key::G3, G13Key::G4],
    &[G13Key::G5, G13Key::G6, G13Key::G7, G13Key::G8],
    &[G13Key::G9, G13Key::G10, G13Key::G11, G13Key::G12],
    &[G13Key::G13, G13Key::G14, G13Key::G15],
    &[G13Key::G16, G13Key::G17, G13Key::G18, G13Key::G19],
    &[G13Key::G20, G13Key::G21, G13Key::G22],
];

impl eframe::App for MonitorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let snapshot = self.state.lock().unwrap().clone();

        egui::TopBottomPanel::top("hd").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("G13 Monitor");
                match &snapshot.connection {
                    Connection::Connected => ui.colored_label(egui::Color32::from_rgb(95, 214, 138), "● Connected"),
                    Connection::Disconnected(why) => ui.colored_label(egui::Color32::from_rgb(220, 90, 90), format!("○ {why}")),
                };
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut active = !self.dry_run.load(Ordering::Relaxed);
                    if ui.selectable_label(active, "Active").clicked() { active = true; }
                    if ui.selectable_label(!active, "Dry-run").clicked() { active = false; }
                    self.dry_run.store(!active, Ordering::Relaxed);
                    ui.label("mode:");
                });
            });
        });

        egui::TopBottomPanel::bottom("ft").show(ctx, |ui| {
            let cfg = self.config.read().unwrap();
            let joy = cfg.joystick()
                .map(|j| format!("joystick: {:?}, deadzone {}", j.mode, j.deadzone))
                .unwrap_or_else(|| "joystick: disabled".to_string());
            ui.label(format!("config.toml · {joy}"));
            if let Connection::Disconnected(_) = &snapshot.connection {
                if ui.button("Retry connection").clicked() {
                    self.start_consumer(ctx.clone());
                }
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let cfg = self.config.read().unwrap();
            ui.horizontal(|ui| {
                // Left: G-key grid in physical rows.
                ui.vertical(|ui| {
                    for row in ROWS {
                        ui.horizontal(|ui| {
                            for &key in row {
                                let pressed = snapshot.pressed.contains(&key);
                                let binding = cfg.get_binding(key).unwrap_or("—");
                                let fill = if pressed { egui::Color32::from_rgb(20, 54, 31) } else { egui::Color32::from_gray(38) };
                                egui::Frame::none().fill(fill).inner_margin(4.0).corner_radius(4.0).show(ui, |ui| {
                                    ui.set_width(58.0);
                                    ui.vertical(|ui| {
                                        ui.strong(format!("{key:?}"));
                                        ui.small(binding);
                                    });
                                });
                            }
                        });
                    }
                });

                ui.separator();

                // Right: joystick panel.
                ui.vertical(|ui| {
                    ui.label("JOYSTICK");
                    let (dz, up, down, left, right) = cfg.joystick()
                        .map(|j| (
                            j.deadzone,
                            j.up.clone().unwrap_or_default(),
                            j.down.clone().unwrap_or_default(),
                            j.left.clone().unwrap_or_default(),
                            j.right.clone().unwrap_or_default(),
                        ))
                        .unwrap_or((30, "w".into(), "s".into(), "a".into(), "d".into()));

                    let size = egui::vec2(140.0, 140.0);
                    let (resp, painter) = ui.allocate_painter(size, egui::Sense::hover());
                    let rect = resp.rect;
                    painter.rect_stroke(rect, 4.0, egui::Stroke::new(1.0, egui::Color32::from_gray(90)), egui::StrokeKind::Inside);
                    // deadzone circle (radius scaled from the 0..255 axis range)
                    let c = rect.center();
                    let dz_r = rect.width() * (dz as f32 / 255.0);
                    painter.circle_stroke(c, dz_r, egui::Stroke::new(1.0, egui::Color32::from_gray(70)));
                    // live position dot
                    let px = rect.left() + rect.width() * (snapshot.joy_x as f32 / 255.0);
                    let py = rect.top() + rect.height() * (snapshot.joy_y as f32 / 255.0);
                    painter.circle_filled(egui::pos2(px, py), 6.0, egui::Color32::from_rgb(127, 224, 160));

                    let hot = egui::Color32::from_rgb(127, 224, 160);
                    let dim = egui::Color32::from_gray(140);
                    let a_left = snapshot.joy_x < 127u8.saturating_sub(dz);
                    let a_right = snapshot.joy_x > 127u8.saturating_add(dz);
                    let a_up = snapshot.joy_y < 127u8.saturating_sub(dz);
                    let a_down = snapshot.joy_y > 127u8.saturating_add(dz);
                    ui.horizontal(|ui| {
                        ui.colored_label(if a_up { hot } else { dim }, format!("↑{up}"));
                        ui.colored_label(if a_down { hot } else { dim }, format!("↓{down}"));
                        ui.colored_label(if a_left { hot } else { dim }, format!("←{left}"));
                        ui.colored_label(if a_right { hot } else { dim }, format!("→{right}"));
                    });
                });
            });
        });
    }
}
```

Remove the now-unused simple `horizontal_wrapped` render from Task 4 (this block replaces it entirely).

- [ ] **Step 2: Build + full test suite**

Run: `cargo build 2>&1 | tail -2` → `Finished`.
Run: `cargo test 2>&1 | tail -5` → `60 passed`.

Note: some egui 0.31 calls above (`rect_stroke` with `StrokeKind`, `corner_radius`, `allocate_painter`) may differ slightly in your resolved eframe version. If a call does not compile, consult current egui docs (Context7 / docs.rs) and adapt the call — the intent (draw a bordered box, a deadzone circle, a filled dot) is what matters.

- [ ] **Step 3: Commit**

```bash
git add src/monitor/mod.rs
git commit -m "feat: physical-layout grid, joystick panel, status footer, Retry

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 6: Milestone doc, roadmap note, and hardware acceptance test

**Files:**
- Create: `milestones/open/gui-monitor.md` (then move to `finished/` on pass)
- Modify: `CLAUDE.md` (roadmap note that the GUI monitor shipped early)

- [ ] **Step 1: Hardware acceptance smoke test (manual — requires the G13 on WinUSB)**

Build and run:

```bash
export PATH="$HOME/.cargo/bin:/c/Strawberry/c/bin:$PATH"
cargo build --release 2>&1 | tail -2
export RUST_LOG=debug
./target/release/g13-driver.exe
```

Confirm ALL of:
- Window opens in **Dry-run** (toggle shows Dry-run selected).
- With Notepad focused, press G-keys → they highlight in the grid with the correct binding label, and **nothing is typed** in Notepad (dry-run).
- Move the stick → the dot moves in the box and the active WASD direction(s) highlight; **nothing typed**.
- Flip to **Active** → now the same inputs inject (G1 → copy, stick up → `w`); flip back to **Dry-run** while holding a direction → the held key releases (no stuck key), injection stops.
- Unplug the G13 → status goes to Disconnected; **Retry connection** appears. Replug, click Retry → status returns to Connected and input resumes.
- `./target/release/g13-driver.exe --headless` → console driver runs as before (no window).

- [ ] **Step 2: Create the milestone file**

Create `milestones/open/gui-monitor.md`:

```markdown
# GUI monitor (dry-run test tool)

- **Status:** finished
- **Target:** (pulled forward from v1.0 GUI at user request)
- **Updated:** 2026-07-03

## Goal
Default-launch egui/eframe window: live G13 monitor (G-keys + joystick) + mapping
preview + Dry-run/Active toggle, so the driver can be tested without injecting into
other Windows apps. Tray + configurator remain future work.

## Outcome
Shipped and hardware-verified. Spec: `docs/superpowers/specs/2026-07-01-gui-monitor-dry-run-design.md`;
plan: `docs/superpowers/plans/2026-07-03-gui-monitor-dry-run.md`.
- `DeviceState` reducer (tested) reconstructs live input from the event stream.
- `runtime` module shares wiring; `--headless` preserves the console driver.
- Consumer thread updates the monitor and injects only when Active; Active→Dry-run
  and disconnect release held keys.
- Physical-layout key grid, joystick box (deadzone + WASD highlight), Retry on disconnect.

## Follow-ups
- System tray + minimize-to-tray + start-in-tray + remember-last-state.
- Automatic reconnect polling (beyond manual Retry).
- M-keys / joystick-click in the monitor (pairs with the M-key decode sub-project).
```

- [ ] **Step 2b: Move the milestone to finished (acceptance passed)**

```bash
git mv milestones/open/gui-monitor.md milestones/finished/gui-monitor.md
```

- [ ] **Step 3: Note the roadmap change in CLAUDE.md**

In `CLAUDE.md`, under the "Roadmap (post-MVP)" section, append a line:

```markdown
> **Note:** the GUI monitor/configurator was partly pulled forward from v1.0 — a
> dry-run input monitor (egui) shipped early to ease hardware testing. See
> `milestones/finished/gui-monitor.md`.
```

- [ ] **Step 4: Commit**

```bash
git add milestones/finished/gui-monitor.md CLAUDE.md
git commit -m "docs: record GUI monitor milestone (hardware-verified) and roadmap note

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Self-Review

**Spec coverage:**
- egui/eframe default window → Tasks 3–5 ✓
- Live monitor of G-keys + joystick → Task 4 (basic) + Task 5 (polish) ✓
- Mapping preview (binding labels, WASD) → Tasks 4/5 ✓
- Dry-run/Active toggle, first-run Dry-run, release-on-transition → Task 4 ✓
- `DeviceState` reducer (tested) → Task 1 ✓
- Shared `runtime` wiring + `--headless` → Tasks 2/3 ✓
- Graceful disconnect + Retry, never-crash → Tasks 4 (consumer/disconnect) + 5 (Retry) ✓
- Physical layout (approved) → Task 5 ✓
- Testing: DeviceState unit tests; rendering/consumer manual-verify → Tasks 1 + 6 ✓
- Milestone/roadmap → Task 6 ✓

**Deviations from spec (deliberate):**
1. Module named `runtime`, not `core` (avoids shadowing std `core`).
2. `runtime` wiring (incl. `spawn_usb_reader`) is manual-verify, not unit-tested — it is I/O whose result depends on device presence (flaky); joins the documented USB/`SendInput` exception. `DeviceState::apply` is the tested surface.

**Placeholder scan:** none — every step has concrete code/commands. GUI drawing calls carry an explicit "adapt to your resolved egui version via docs" note where the API is version-sensitive (not a placeholder — the intent and representative code are given).

**Type consistency:** `DeviceState{pressed,joy_x,joy_y,connection}`, `Connection::{Connected,Disconnected(String)}`, `MonitorApp{config,state,dry_run}`, `consumer_loop(...)`, `start_consumer(ctx)`, `G13Key::ALL`, `runtime::{load_config_and_watch,spawn_usb_reader,run_headless}`, `monitor::run` — used consistently across tasks.
