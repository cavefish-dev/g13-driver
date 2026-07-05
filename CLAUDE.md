# CLAUDE.md

Guidance for AI agents working in this repository.

## What this is

An open-source replacement driver for the **Logitech G13** gaming keypad (official
drivers are abandonware). It runs as a **Windows console app** in Rust: reads raw USB
HID reports from the G13, decodes G-key presses, and injects virtual keystrokes via
Win32 `SendInput`, using a TOML-configured key map with hot-reload.

Full design rationale: `docs/design-spec.md`. Original step-by-step build: `docs/implementation-plan.md`.

## Status

**MVP is complete and committed** (10 commits, 28 passing unit tests, release binary builds clean).
What works: G-key → keyboard-shortcut mapping, TOML config, config hot-reload, Windows-only.

**Not yet verified on hardware** — no physical G13 has been connected. The USB read path
(`src/usb.rs`) and keystroke injection (`src/injector/windows.rs`) have *no* unit tests by
design; they need a manual smoke test. See `milestones/open/hardware-bringup.md` — this is
the most important open task.

Track all work under `milestones/` (see "Milestones" below).

## Toolchain — READ THIS FIRST (non-obvious)

This machine has **no MSVC C++ build tools**. The project is built with the **GNU**
Rust toolchain, not the usual MSVC one:

- Active toolchain: `stable-x86_64-pc-windows-gnu` (set as rustup default).
- `rusb` builds bundled **libusb** from C source via `cc`, which needs a C compiler.
  The one used is **MinGW-w64 gcc from Strawberry Perl** at `C:\Strawberry\c\bin\gcc.exe`.
- `cargo`/`rustc` live in `%USERPROFILE%\.cargo\bin` (i.e. `~/.cargo/bin`).

A fresh terminal *should* have both on PATH (rustup adds `~/.cargo/bin`; Strawberry is on
the system PATH). If `cargo: command not found` or a libusb/`gcc` link error appears, prepend
them explicitly. In Git Bash:

```bash
export PATH="$HOME/.cargo/bin:/c/Strawberry/c/bin:$PATH"
```

In PowerShell:

```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;C:\Strawberry\c\bin;$env:Path"
```

Do **not** switch to the MSVC target unless you first install the VS C++ build tools.

## Build & test

```bash
cargo test            # 28 unit tests (protocol, injector, config, dispatcher)
cargo build           # debug build
cargo build --release # -> target/release/g13-driver.exe
cargo run             # runs the driver; needs config.toml in CWD + a G13 on WinUSB
RUST_LOG=debug cargo run   # verbose logging (env_logger)
```

The full dependency tree (incl. building libusb) takes ~1–2 min on a cold build; warm
builds are a few seconds.

## Architecture

Layered so platform-specific code is isolated behind a trait — the future Linux port is a
struct swap, not a rewrite.

```
G13 USB --> UsbReader --> ReportParser --> Dispatcher --> KeyInjector(trait) --> OS input
              (thread)    bytes->Event    Event+Config    Windows: SendInput
```

| File | Responsibility |
|------|---------------|
| `src/main.rs` | wires components, spawns USB + config-watch threads, runs dispatch loop |
| `src/protocol.rs` | `G13Key`, `G13Event`, `ReportParser` (8-byte report bitmask -> events) |
| `src/config.rs` | `Config` / `RawConfig`: TOML load, `G13Key` mapping, `get_binding` |
| `src/dispatcher.rs` | routes `G13Event` -> `KeyInjector` via `Config` (behind `Arc<RwLock>`) |
| `src/injector/mod.rs` | `KeyCombo`, `Modifier`, `KeyInjector` trait, combo string parser |
| `src/injector/key_map.rs` | `build_key_map()` — string -> Win32 VKey table |
| `src/injector/windows.rs` | `WindowsInjector` — `SendInput`, `#[cfg(windows)]` |
| `src/usb.rs` | `UsbReader` — opens G13 (VID `0x046D`/PID `0xC21C`), reads endpoint `0x81` |
| `config.toml` | example bindings (shipped, hot-reloaded) |
| `docs/zadig-setup.md` | one-time WinUSB driver swap guide |

## Conventions

- **TDD.** Every pure-logic module was built test-first (RED -> GREEN). Keep it: add a
  failing test before implementing, run it to confirm it fails, then make it pass. USB and
  `SendInput` code is the documented exception (manual verification only).
- **Error policy:** injection failures log a warning and continue — a missed keypress is
  recoverable, a crashed driver is not. Don't `panic!`/`unwrap()` in the runtime path.
- **Platform isolation:** all OS-specific code stays behind `#[cfg(...)]` inside
  `src/injector/`. Don't leak Win32 types into `dispatcher`/`config`/`protocol`.
- One focused commit per logical change; imperative subject line (matches existing history).
- `/target` is gitignored. Line endings are CRLF on checkout (Git autocrlf warnings are harmless).

## Roadmap (post-MVP)

Phases are tracked as milestone files. Summary from the spec:
v0.2 joystick->WASD/mouse + M-key profiles + Windows Service · v0.3 macros + shell commands ·
v0.4 LCD (160x43) · v0.5 RGB backlight · v1.0 Linux (udev+uinput) + GUI configurator.

> **Note:** the GUI was partly pulled forward from v1.0 — a default-launch **dry-run
> input monitor** (egui/eframe) shipped early to ease hardware testing without
> injecting into other apps. Run with no args for the GUI; `--headless` for the
> console driver. See `milestones/finished/gui-monitor.md`. The GUI also has a
> **Profiles** tab (switch M-key profiles) and a **Bindings** tab that edits the active
> profile's key bindings and saves them to the profile file
> (`milestones/finished/gui-bindings-editor.md`).

## Milestones

Work is tracked as one markdown file per milestone, filed by lifecycle state under
`milestones/`. See `milestones/README.md` for the workflow. States: `open/` (not started),
`ongoing/` (in progress), `finished/` (done, kept for reference), `archived/` (dropped or
superseded). Move the file between folders as state changes; update its checklist as you go.
