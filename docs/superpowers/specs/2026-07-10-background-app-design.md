# Background app (tray, auto-start, single-instance) — design

- **Status:** approved (design)
- **Date:** 2026-07-10
- **Scope:** Turn the G13 driver into a background app: run without a console flash, live in the
  Windows system tray with a status icon + menu, hide-to-tray on close/minimize (driver keeps
  running), auto-start at login (opt-in), enforce a single instance, and resume the last
  Active/Dry-run mode. This is sub-project #1 of the MVP push (the others: GitHub Actions CI
  driven by a `version.txt` semver file; auto-update from GitHub Releases).

## Motivation

A gaming keypad driver should run quietly in the background from login, not require a console
window or a manually-managed GUI. A true Windows **service** is the wrong tool: services run in
session 0, isolated from the interactive desktop, so `SendInput` from a service cannot reach the
user's session. The correct pattern for an input-injection app is a **user-session background
app** that auto-starts at login and lives in the tray — which is what this delivers.

## Architecture / lifecycle

The core change is **decoupling the window from the process**.

- Today closing the eframe window ends the process (and the driver). After this change the process
  lifetime is owned by the **tray**, and the window is a summonable/dismissable front-end.
- The **driver threads are unaffected**: `start_consumer` already spawns the USB reader and
  `consumer_loop` on their own threads, independent of window visibility. Hiding the window does
  not touch them, so **Active injection keeps working while hidden**. No driver restructuring.
- **Close (X) and Minimize** are intercepted in eframe `update()` (`close_requested()` →
  `ViewportCommand::CancelClose` + hide; `minimized` → hide) so both gestures mean "keep running,
  get out of the way." The process exits only via the tray **Quit** (or an unrecoverable error).
- **Hidden-wake:** while hidden, egui does not naturally run `update()` (no repaints). Tray clicks
  are therefore handled through `tray-icon`'s **event handler**, invoked on the UI/event-loop
  thread when a tray message arrives; it mutates shared state and calls
  `send_viewport_cmd(Visible(true))` / `request_repaint()` so Show/Active work while hidden and
  idle. A disconnect already calls `request_repaint()` in `consumer_loop`, so the status icon also
  updates while hidden.

```
tray menu/icon click ─(tray-icon handler, UI thread)─► mutate shared state ─► send_viewport_cmd / request_repaint
window close/minimize ─(update())─► CancelClose + hide
USB reader + consumer_loop ───(own threads, unaffected by window)───► inject when Active
```

## Tray icon, menu & interactions

**Status icon (3 states, procedurally generated — no image asset, no `image` dep).** Two 32×32
RGBA buffers per state built in code; swapped when the effective state changes. Precedence
**problem > mode**:

| State | Condition | Tooltip |
|---|---|---|
| **Red** | `DeviceState.connection != Connected` (not found, WinUSB/driver not ready, open error, disconnect) | `G13 — not connected` |
| **Green** | connected **and** Active | `G13 — Active` |
| **Grey** | connected **and** Dry-run | `G13 — Dry-run` |

The icon is recomputed each frame in `update()` from `(connection, mode)` via a pure
`icon_state(connection, mode) -> IconState` function and swapped only on change.

**Menu (right-click), stable item IDs:**
- **Show / Hide window** — toggle viewport visibility.
- **Active** — checkable; reflects and flips the live Dry-run/Active `AtomicBool` (shared with the
  Settings/Monitor toggle; persisted — see below).
- **Start at login** — checkable; reads/writes the registry Run key (below).
- ── separator ──
- **Quit** — the only path that exits the process (`ViewportCommand::Close`, after the existing
  `dispatcher.release_held()` shutdown path runs so nothing sticks).

**Interactions:** left-click / double-click the tray icon → toggle the window; right-click → menu;
window Close (X) / Minimize → hide to tray. Menu checkmarks (Active, Start at login) are synced
from the true state on change / before the menu shows, so tray and Settings never disagree.

## Auto-start & single instance

**Auto-start** — per-user registry Run key `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`,
value name `g13-driver`, data `"<full-exe-path>" --minimized`. The **Start at login** toggle (tray
+ Settings) writes the value when enabled, deletes it when disabled; both surfaces read the key
live. Off by default. `--minimized` makes a login launch start hidden in the tray; a manual launch
(no flag) shows the window — so no separate "start minimized" setting is needed. Implemented with
the `winreg` crate.

**Single instance** — on startup create a named mutex `Local\g13-driver-singleton`. If it already
exists, another instance owns the USB device, so the new process **signals the running instance to
show its window and then exits**. Signaling: a named event the first instance waits on in a small
thread; when set, that thread calls `send_viewport_cmd(Visible(true) + Focus)` + `request_repaint()`.
This prevents the two-process USB-claim conflict (libusb lets only one process hold the device).

## Persisted mode & no-console

**Persisted mode (`config.toml`).** The manifest gains:

```toml
[app]
start_active = false   # last Active/Dry-run choice (false = Dry-run); first run defaults false
```

- Startup reads `start_active` to initialize the live `AtomicBool` (a login launch resumes the last
  mode — no manual flip).
- Toggling Active/Dry-run writes `start_active` back to the manifest, **preserving**
  `profiles_dir`/`m1`/`m2`/`m3`.
- **Watcher interaction:** the self-write trips the hot-reload watcher, but the reload is benign —
  the live `AtomicBool` is the session's source of truth (not re-derived from `ProfileSet` after
  startup), and the reload already preserves the active M-key. Auto-start lives in the registry
  (no drift); `start_minimized` is conveyed by the `--minimized` flag (no config key).
- **Mode / config mode:** `[app]` is read via `RawManifest`, so `start_active` works in both
  manifest and legacy single-profile mode. Persistence (write-back) targets the manifest, which is
  the shipped default. In legacy single-profile mode `start_active` is still *read* if present;
  writing it back preserving the profile's `[keys]`/`[joystick]`/`[repeat]` is a follow-up (legacy
  is the compat path, not the default) — a write failure there just logs and continues.

**No console flash.** `#![windows_subsystem = "windows"]` at the crate root so the GUI never
flashes a console. For `--headless`, `main` calls `AttachConsole(ATTACH_PARENT_PROCESS)` at startup
so `env_logger` output appears when run from a terminal (no parent console → headless still runs,
logs go nowhere). Existing `--headless` behavior is otherwise unchanged.

## Components

| File | Responsibility |
|------|---------------|
| `src/tray.rs` (new) | `TrayIcon` creation, procedural 3-state RGBA icons, menu + stable IDs, event handler → actions; pure `icon_state()`; `#[cfg(windows)]` |
| `src/autostart.rs` (new) | read/set/clear the HKCU Run key via `winreg`; pure `run_command(exe) -> String`; `#[cfg(windows)]` |
| `src/single_instance.rs` (new) | named mutex (detect) + named event (activate existing); `#[cfg(windows)]` |
| `src/main.rs` | `#![windows_subsystem = "windows"]`; parse `--minimized`; `AttachConsole` for `--headless`; single-instance guard before launch; pass initial-hidden + mode into `monitor::run` |
| `src/monitor/mod.rs` | hold the tray handle; `update()` intercepts close/minimize→hide, drains tray actions, recomputes+swaps the status icon; Settings tab wires real **Start at login** (registry) + reflects it; initial mode from `[app]`; accept a `start_minimized` arg |
| `src/config.rs` | parse `[app] start_active` (default false) + persist it to the manifest preserving other keys |
| `Cargo.toml` | add `tray-icon`, `winreg` under `[target.'cfg(windows)'.dependencies]` |

## Error handling

Every OS touchpoint (tray create, registry, mutex/event, `AttachConsole`) **logs a warning and
continues**. The app degrades gracefully: no tray → plain window; autostart write fails → the
toggle reports it and the app still runs. No `panic!`/`unwrap()` in these paths — consistent with
the project's error policy.

## Testing

**Unit (TDD) — pure logic:**
- `config.rs`: `[app]` parse; `start_active` defaults to `false` when absent; manifest round-trip
  persists `start_active` while preserving `profiles_dir`/`m1`/`m2`/`m3`.
- `autostart.rs`: `run_command(exe)` builds exactly `"<exe>" --minimized`.
- `tray.rs`: `icon_state(connection, mode)` precedence — problem > active > dry-run (red when
  disconnected even if Active; green when connected+Active; grey when connected+Dry-run).

**Manual-verify (documented OS-integration exception — like USB/`SendInput`):** tray creation,
registry read/write, named mutex/event, `windows_subsystem`/`AttachConsole`, and the eframe
close/minimize/hide wiring.

**Manual smoke test:** launch → window shows; Close (X) → hides to tray, driver keeps injecting
(press keys, still work); tray left-click → toggles the window; menu **Active** → flips mode, icon
green↔grey; unplug the G13 → icon red + tooltip; **Start at login** → registry value appears, and
disappears when unticked; relaunch while running → focuses the existing window, no second process;
`--headless` from a terminal → still logs; `--minimized` → starts hidden in the tray.

## Out of scope (follow-ups)
- Auto-reconnect while hidden (for now: red icon → open window → Retry).
- Balloon/toast notifications.
- Profile switching (M1/M2/M3) from the tray menu.
- A true Windows Service (explicitly dropped — session-0 injection problem).
