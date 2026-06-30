# MVP — console driver

- **Status:** finished
- **Target:** v0.1
- **Updated:** 2026-06-30

## Goal
Windows console app that reads G-key presses from a G13 over USB and injects virtual
keystrokes from a TOML key map, with config hot-reload.

## Tasks
- [x] Project scaffold (Cargo, deps, gitignore)
- [x] `protocol`: `G13Key` / `G13Event` / `ReportParser` bitmask decode (TDD)
- [x] `injector`: `KeyCombo` / `Modifier` / `KeyInjector` trait + combo parser (TDD)
- [x] `key_map`: string -> Win32 VKey table (TDD)
- [x] `config`: TOML load + `G13Key` mapping + `get_binding` (TDD)
- [x] `dispatcher`: route `G13Event` -> injector via config (TDD)
- [x] `injector/windows`: `SendInput` implementation
- [x] `usb`: `UsbReader` over rusb/libusb (VID 0x046D / PID 0xC21C, endpoint 0x81)
- [x] `main`: wire threads + dispatch loop + config hot-reload
- [x] Example `config.toml` + `docs/zadig-setup.md`

## Acceptance
- [x] `cargo test` — 28 passing
- [x] `cargo build --release` — clean
- [ ] End-to-end on hardware — **deferred to `open/02-hardware-bringup.md`** (no G13 available at build time)

## Notes
- Built on the **GNU** Rust toolchain (no MSVC tools on the machine); libusb compiled
  with Strawberry Perl's MinGW gcc. See repo `CLAUDE.md` "Toolchain".
- All pure-logic modules built test-first. USB + `SendInput` paths have no unit tests by
  design — verified manually instead.
