# g13-driver

[![CI](https://github.com/cavefish-dev/g13-driver/actions/workflows/ci.yml/badge.svg)](https://github.com/cavefish-dev/g13-driver/actions/workflows/ci.yml)
[![Latest release](https://img.shields.io/github/v/release/cavefish-dev/g13-driver)](https://github.com/cavefish-dev/g13-driver/releases/latest)
[![License: GPL-3.0-or-later](https://img.shields.io/badge/license-GPL--3.0--or--later-blue)](LICENSE)

An open-source replacement driver for the **Logitech G13** gaming keypad on Windows. Logitech's
official software is abandonware; this brings the G13 back to life with a small, modern app.

## What it does

- Remap the **G-keys** (G1–G22), the two **thumb buttons**, and the **joystick click** to any key
  or keyboard shortcut (e.g. `ctrl+c`, `alt+tab`).
- Map the **joystick** to WASD (hold-to-move).
- **Profiles** on the M1/M2/M3 keys — switch whole binding sets on the fly.
- Runs quietly in the **system tray**; optional **auto-start at login**.
- Configure everything in a simple GUI — no config files required.

## Requirements

- Windows 10 or 11
- A Logitech G13 keypad
- About 5 minutes for a one-time driver setup

## Quick start

### 1. Download

Grab the latest `g13-driver-vX.Y.Z-windows-x64.zip` from the
[**Releases**](https://github.com/cavefish-dev/g13-driver/releases/latest) page and **extract the
whole folder**. Keep `g13-driver.exe`, `config.toml`, and the `profiles/` folder together.

### 2. One-time driver setup (Zadig)

Windows needs to let this app talk to the G13 over USB. You do this once with a free tool called
**Zadig**, which installs the generic **WinUSB** driver on the G13. It takes a minute and is
reversible.

Follow the step-by-step guide: **[docs/zadig-setup.md](docs/zadig-setup.md)**.

> This does not delete Logitech's software — it just points the G13 at WinUSB so g13-driver can read
> it. You can switch back anytime.

### 3. Run it

Double-click **`g13-driver.exe`**.

Because the app isn't code-signed yet, Windows **SmartScreen** may show *"Windows protected your
PC"*. Click **More info → Run anyway**. This is expected for a small open-source app; the code is
public in this repo and signing is on the roadmap.

### 4. Turn it on

The app starts in **Dry-run** mode — it shows what you press but injects nothing (safe for testing).
When you're ready, switch to **Active** (top-right toggle, or the tray menu) and your bindings take
effect.

## Using it

- **Tray app:** closing or minimizing the window **hides it to the tray** — the driver keeps
  running. Use **Quit** in the tray menu to actually exit.
- **Status icon:** green = Active, grey = Dry-run, **red = the G13 isn't connected** (run Zadig / check
  the cable). It auto-reconnects when you plug the G13 back in.
- **Auto-start at login:** enable it in **Settings** (or the tray menu) so the driver is ready every
  time you log in.

## Configuring

Everything is done in the GUI — you never have to touch a file:

- **Bindings tab:** click a key row, type the key or shortcut (e.g. `ctrl+c`), tick **repeat** to make
  it auto-repeat while held, then **Save**.
- **Profiles tab:** M1/M2/M3 each load a profile; press the M-key (or click the slot) to switch.
- **Joystick / auto-repeat:** joystick→WASD and repeat timing are configurable too.

Power users can hand-edit the TOML config files next to the exe — see
**[docs/configuration.md](docs/configuration.md)** for the full reference.

## Updating

For now, download the newer release zip and replace your files (keep your edited `config.toml` /
`profiles/` if you customized them). Built-in auto-update is on the roadmap.

## Troubleshooting

| Problem | Fix |
|---|---|
| Tray icon is **red** / "not connected" | Run the [Zadig setup](docs/zadig-setup.md); check the USB cable. |
| Keys do nothing | You're in **Dry-run** — switch to **Active**. |
| "Windows protected your PC" | SmartScreen on an unsigned app — **More info → Run anyway**. |
| A binding won't save | The key name is invalid — see [configuration.md](docs/configuration.md) for valid names. |

## Building from source

Windows, with the **GNU** Rust toolchain (not MSVC) and a MinGW-w64 `gcc` (for `rusb`'s bundled
libusb):

```sh
rustup default stable-x86_64-pc-windows-gnu
cargo test
cargo build --release   # -> target/release/g13-driver.exe
```

See **[CONTRIBUTING.md](CONTRIBUTING.md)** for the full developer setup and workflow.

## Roadmap

Roughly where things are headed (direction, not promises):

- **Done:** key/thumb/stick remapping, joystick→WASD, M-key profiles, GUI monitor + bindings editor,
  hold-means-hold + media keys, auto-repeat, tray background app, CI + GitHub releases.
- **Next:** in-app auto-update from GitHub Releases.
- **Later:** macros + shell commands, the G13 LCD (160×43), RGB backlight, Linux support, and a
  standalone GUI configurator.

## Contributing & license

Contributions welcome — see [CONTRIBUTING.md](CONTRIBUTING.md). Licensed under
**GPL-3.0-or-later** (see [LICENSE](LICENSE)); contributions are accepted under the same license.

Thanks to the broader community whose G13 reverse-engineering made an open driver possible.
