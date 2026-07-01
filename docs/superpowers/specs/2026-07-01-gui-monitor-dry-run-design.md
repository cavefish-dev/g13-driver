# GUI monitor (dry-run test tool) — design

- **Status:** approved (design)
- **Date:** 2026-07-01
- **Scope:** A default-launch `egui`/`eframe` window that shows the G13's live input
  (G-keys + joystick) and its configured mapping (preview), with a **Dry-run/Active**
  toggle so the driver can be exercised **without injecting keystrokes into other apps**.
  System tray + start-in-tray is explicitly a **follow-on sub-project**, not this one.

## Motivation

Testing the driver currently means injecting real keystrokes and observing side effects in
other windows (which caused focus-stealing chaos during bring-up). A live visual monitor with
injection gated off removes that: you *see* what the device sends and what it *would* inject,
with no Windows side effects. This becomes the default face of the app; the driver still runs
(and injects, when Active) behind it.

## Goals

- Launch by default into a window (no flag) showing live device state.
- Show each G-key and its `config.toml` binding; highlight keys currently pressed.
- Show joystick X/Y as a live 2D position with deadzone + WASD mapping; highlight active
  directions.
- **Dry-run/Active toggle**, defaulting to **Dry-run** on first launch. Dry-run = monitor
  only, no `SendInput`. Active = normal injection.
- Never crash on device problems; surface connection status and offer a manual **Retry**.
- Preserve a windowless `--headless` mode (the current console driver, always Active).

## Non-goals (this sub-project)

- System tray icon, minimize-to-tray, start-in-tray (next sub-project).
- Automatic reconnect polling (manual **Retry** button only this round).
- Editing bindings in the GUI / profile management (the full configurator, a later,
  v1.0-sized effort).
- M-keys / joystick-click in the monitor — the driver does not decode bytes 6/7 yet
  (sub-project 2). `DeviceState` is designed to extend for them later.

## Approach

**Framework: `egui`/`eframe`.** Immediate-mode is a natural fit for a live monitor (each
frame renders current state), pure Rust, builds on the project's `windows-gnu` toolchain,
and is cross-platform (helps the eventual Linux port). Chosen over `iced` (more boilerplate
for live state) and `Tauri` (pulls WebView2 + a JS toolchain — against the minimal-dependency
ethos).

## Architecture & threading

GUI runs on the **main thread** (winit requirement). Input processing runs on **background
threads**, decoupled from frame rate, so injection latency does not depend on repainting.

```
USB ─► UsbReader(thread) ─► G13Event channel ─► Consumer(thread) ─► Dispatcher ─► Injector
                                                    │  (inject only when Active)
                                                    ├─► DeviceState (Arc<Mutex>)  [display]
                                                    └─► ctx.request_repaint()
                                                                      ▲
        eframe App (main thread): snapshot DeviceState + read Config each frame, render ─┘
```

- **Consumer thread** drains `G13Event`s: applies each to `DeviceState` (for display), and —
  **only when `dry_run == false`** — forwards to `Dispatcher` to inject. Calls
  `ctx.request_repaint()` on change so the UI updates promptly.
- **GUI thread** performs no injection. Each frame it snapshots `DeviceState` and reads
  `Config` for the mapping labels, then renders. The Dry-run/Active toggle writes a shared
  `AtomicBool`.
- **Dry-run gating & safety:** the consumer reads the `AtomicBool` each event. On an
  Active→Dry-run transition it calls `dispatcher.release_held()` first, so switching to
  Dry-run never leaves a joystick key stuck down. First launch = Dry-run.
- **Reuse:** `Config` (+ its hot-reload watch thread), `UsbReader`, `ReportParser`,
  `Dispatcher` are used unchanged.

## Components

### `src/core.rs` (new) — shared startup wiring
Lifts startup out of `main` so GUI and headless share one path:
- `load_config_and_watch(path) -> Arc<RwLock<Config>>` — load + spawn the existing watcher.
- `spawn_usb_reader() -> Result<Receiver<G13Event>>` — open `UsbReader`, spawn the reader
  thread, return the event channel. Returns `Err` (does not `exit`) so the GUI can display it.

### `src/device_state.rs` (new) — reducer (pure, TDD)
- `struct DeviceState { pressed: HashSet<G13Key>, joy_x: u8, joy_y: u8, connection: Connection }`
- `enum Connection { Connected, Disconnected(String) }`
- `fn apply(&mut self, event: &G13Event)` — `KeyDown` inserts, `KeyUp` removes,
  `JoystickMove` sets x/y. Extensible for M-keys later. This is the primary unit-test surface.
- `Default`/`new()` starts empty, `joy_x = joy_y = 127` (centered), `Disconnected("connecting")`
  until the first event / explicit status.

### `src/monitor/` (new) — the eframe GUI (manual-verify)
- `MonitorApp: eframe::App` holds `Arc<Mutex<DeviceState>>`, `Arc<RwLock<Config>>`,
  `Arc<AtomicBool> dry_run`. Constructor grabs the egui `Context` and spawns the consumer
  thread (moving in the `Receiver`, `Dispatcher`, `dry_run`, `DeviceState`, `ctx`).
- `update()` renders per frame: header (title, connection pill, Dry-run/Active toggle),
  **physical G13 layout** of G-keys (label + binding, pressed = highlighted), joystick panel
  (position dot in a box, deadzone circle, WASD labels with active direction highlighted),
  status footer (config path, last reload, joystick settings), and a **Retry connection**
  button when disconnected.
- `run() -> Result<()>` — builds shared state, calls `eframe::run_native`.
- **Connection lifecycle:** on startup and on each **Retry**, the app calls
  `spawn_usb_reader`; on `Ok` it sets `connection = Connected` and starts the consumer
  thread; on `Err(reason)` it sets `connection = Disconnected(reason)` and starts no
  consumer. When a running consumer's channel closes (unplug), it sets
  `connection = Disconnected("device disconnected")` and releases held keys.

### `src/main.rs` — mode selection
- Default (no args) → `monitor::run()`.
- `--headless` → the current console loop (always Active) via `core` (release-held on exit
  preserved).

### `Cargo.toml`
- Add `eframe` (pulls winit/glow; pure Rust; builds on the GNU toolchain). eframe is
  Windows-agnostic; keep it a normal dependency (the GUI has no Win32 types — injection stays
  behind `#[cfg(windows)]` in `src/injector/`).

## UI layout (approved: physical layout)

Keys arranged to mirror the physical G13 (rows G1–G7 / G8–G14 / G15–G19 / G20–G22, matching
the device), each a cell with the G-label and its binding, highlighted green while pressed.
Joystick panel to the side: a square with a live position dot, a dashed deadzone circle, and
WASD labels around it with the active direction(s) highlighted. Header carries the connection
pill and the prominent Dry-run/Active toggle; footer shows config path / reload time /
joystick mode+deadzone.

## Error handling & connection

- **G13 not found at startup:** window opens anyway, status shows
  `Disconnected — G13 not found (plug in + WinUSB via Zadig)`, grid renders idle, **Retry**
  button offered. Never exits.
- **Unplugged while running:** USB read errors → reader thread ends → channel closes →
  consumer sets `connection = Disconnected`, calls `release_held()`, status pill goes red.
  **Retry** re-attempts `spawn_usb_reader`.
- **Injection errors (Active):** unchanged — `log::warn!` and continue.
- **Dry-run default** means a misconfigured `config.toml` cannot fire shortcuts until the user
  deliberately switches to Active.

## Testing

- **Unit (TDD):** `DeviceState::apply` — KeyDown inserts; KeyUp removes; KeyUp of an unpressed
  key is a no-op; JoystickMove updates x/y; multiple simultaneous keys tracked; connection
  transitions. Light `core` tests where meaningful (e.g. `spawn_usb_reader` returns `Err`, not
  panic, with no device).
- **Manual-verify (documented exception):** `MonitorApp` rendering; Dry-run/Active gating.
- **Manual smoke test (acceptance):** no-arg launch opens window in Dry-run; pressing G-keys
  lights them with correct bindings; moving the stick moves the dot and highlights WASD — with
  **zero** injection into other apps; toggling Active resumes injection; toggling back releases
  held keys (no stuck key); unplug/replug exercises status + Retry; `--headless` still runs the
  console driver.

## Follow-ups (tracked for later sub-projects)
- System tray + minimize-to-tray + start-in-tray + remember-last-state.
- Automatic reconnect polling (beyond manual Retry).
- M-keys / joystick-click in the monitor (pairs with sub-project 2's byte 6/7 decode).
- Ctrl+C stuck-key handler (already tracked from the joystick sub-project; the GUI's
  Dry-run default mitigates it during testing).
