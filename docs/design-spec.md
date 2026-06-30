---
name: g13-driver
description: Open-source Logitech G13 driver for Windows (and future Linux) — virtual keyboard with TOML-configured key mappings
metadata:
  type: project
---

# G13 Driver — Design Spec

**Date:** 2026-05-28  
**Status:** Approved  
**Language:** Rust  
**Target:** Windows (ARM64 + x86_64), Linux future phase

## Overview

Open-source replacement driver for the Logitech G13 gaming keypad, whose official drivers are abandonware. The driver runs as a **console application** for MVP (promoted to a Windows Service in v0.2), reads raw USB HID reports from the G13, and injects virtual keystrokes into the OS using a TOML-configured mapping. No kernel driver required — uses libusb (via WinUSB/Zadig) for USB access and Win32 `SendInput` for keystroke injection.

The architecture is intentionally layered so platform-specific code is isolated behind a trait, making the Linux port a thin swap rather than a rewrite.

---

## Architecture & Data Flow

```
[G13 USB device]
      │  raw 8-byte HID reports (rusb/libusb + WinUSB)
      ▼
  [UsbReader]          blocking read loop on dedicated thread
      │  raw [u8; 8]
      ▼
  [ReportParser]       bitmask decode → G13Event (KeyDown/KeyUp + which key)
      │  G13Event
      ▼
  [Dispatcher]         looks up key in Config → KeyAction
      │  KeyAction
      ▼
  [KeyInjector trait]
      ├── Windows: SendInput (Win32)
      └── Linux:   uinput (future)
      ▼
  [OS input stack]
```

`Config` is loaded at startup, watched for file changes (hot-reload via `notify` crate), and consulted by `Dispatcher` on every event. It is not in the hot path — just a lookup.

---

## USB Layer

**Device:** VID `0x046D`, PID `0xC21C` (Logitech G13).  
The driver detects the device by these IDs at startup and polls until found, so plug-in after launch works.

**Windows setup (one-time):** The user runs [Zadig](https://zadig.akeo.ie/) to replace the G13's Windows HID driver with **WinUSB**. This is required once per machine. Documented step-by-step in `docs/zadig-setup.md`.  
**Linux setup (future):** A udev rule grants access without any GUI tool.

**Reading:** `rusb` opens the device, claims interface 0, reads interrupt endpoint `0x81` in a blocking loop. Each report is 8 bytes. Runs on a dedicated thread.

**Report format** (from g13d community reverse-engineering):

| Byte | Content |
|------|---------|
| 0    | Report ID |
| 1–3  | G-key bitmask (G1–G22 across 22 bits) |
| 4    | M-keys + joystick button |
| 5    | Joystick X (ignored in MVP) |
| 6    | Joystick Y (ignored in MVP) |
| 7    | Unused |

**G13Event type:**
```rust
enum G13Event {
    KeyDown(G13Key),
    KeyUp(G13Key),
}
```

---

## Config Format

Single `config.toml` in the binary's directory. Override with `--config <path>`.

**MVP (flat string bindings):**
```toml
[keys]
G1  = "ctrl+c"
G2  = "ctrl+v"
G3  = "f5"
G4  = "alt+tab"
G5  = "windows+d"
G22 = "shift+ctrl+esc"
```

- Key names are case-insensitive.
- Modifier order is irrelevant (`ctrl+shift+f5` = `shift+ctrl+f5`).
- Unmapped keys are silently ignored.

**Future-compatible extended form** (non-breaking — simple string bindings continue to work):
```toml
[keys.G2]
type     = "macro"
sequence = ["hello", " ", "world", "enter"]

[keys.G3]
type = "command"
run  = "notepad.exe"
```

**Hot-reload:** `notify` crate watches `config.toml`. On change, re-parse and atomically swap behind `Arc<RwLock<Config>>`. No restart needed.

---

## Key Injector Layer

```rust
trait KeyInjector: Send + Sync {
    fn press(&self, combo: &KeyCombo) -> Result<()>;
}
```

**Windows (`#[cfg(windows)]`):** `SendInput` from `windows-sys`. Sends modifier-down, key-down, key-up, modifier-up as a single `SendInput` call (atomic from the OS's perspective — avoids race conditions with fast apps).

**Linux (`#[cfg(target_os = "linux")]`, future):** `/dev/uinput` via the `uinput` crate. Same trait, different struct. Zero changes to any other component.

**Key name mapping:** `key_map.rs` — a lookup table from strings (`"f5"`, `"ctrl"`, `"windows"`) to Win32 `VIRTUAL_KEY` constants.

**Error policy:** `SendInput` failure logs a warning and continues. A missed keypress is recoverable; a crashed service is not.

---

## Project Structure

```
g13-driver/
├── Cargo.toml
├── config.toml              # example config, ships with binary
├── src/
│   ├── main.rs              # wires components, starts USB thread
│   ├── usb.rs               # UsbReader: find device, read reports
│   ├── protocol.rs          # ReportParser: bytes → G13Event
│   ├── config.rs            # Config: TOML load + hot-reload watcher
│   ├── dispatcher.rs        # Dispatcher: G13Event + Config → KeyAction
│   └── injector/
│       ├── mod.rs           # KeyInjector trait + KeyCombo type
│       ├── key_map.rs       # string → VKey lookup table
│       ├── windows.rs       # SendInput impl
│       └── linux.rs         # uinput impl (future)
└── docs/
    └── zadig-setup.md       # step-by-step Zadig guide
```

---

## Dependencies (Cargo.toml)

| Crate | Purpose |
|-------|---------|
| `rusb` | libusb wrapper (USB read) |
| `serde` + `toml` | config parsing |
| `windows-sys` | Win32 SendInput |
| `anyhow` | error handling |
| `log` + `env_logger` | logging |
| `notify` | config file watching (hot-reload) |
| `uinput` | Linux key injection (future) |

---

## Phases

| Phase | Scope |
|-------|-------|
| **MVP** | G-keys → keyboard shortcuts, TOML config, hot-reload, Windows only |
| **v0.2** | Joystick → WASD / mouse, M-key profile switching, Windows Service install |
| **v0.3** | Macro sequences, shell command bindings |
| **v0.4** | LCD display (160×43px) — active profile, system info |
| **v0.5** | RGB backlight control |
| **v1.0** | Linux port (udev + uinput), ARM cross-compilation, GUI configurator |

---

## Non-Goals (MVP)

- GUI configuration tool
- Cloud sync or profiles
- Logitech GHub compatibility
- Macro recording (type then save)
- Any kernel-mode driver code
