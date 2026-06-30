# Hardware bring-up & smoke test

- **Status:** finished
- **Target:** v0.1 (verification)
- **Updated:** 2026-06-30

## Goal
Verify the MVP actually works against a physical G13. This is the one piece the unit
tests can't cover (USB read + `SendInput` injection). **Do this before any new feature.**

## Tasks
- [x] Connect the G13 via USB.
- [x] Run Zadig once to replace the G13's HID driver with WinUSB (`docs/zadig-setup.md`).
- [x] `RUST_LOG=debug cargo run` from a dir containing `config.toml`; confirm
      `g13-driver running` and no `G13 not found` / claim-interface errors.
- [x] Open Notepad, type text, press **G1** -> text copied (Ctrl+C); **G2** -> pastes (Ctrl+V).
- [x] Confirm no `SendInput returned 0` warnings in the log.
- [x] Edit `config.toml` (e.g. G5 `"f5"` -> `"ctrl+p"`), save -> log shows `config reloaded`.
- [x] Verify the G-key bit mapping matches the real device (esp. G8/G9 byte boundary,
      G22 high bit). **Fixed `ReportParser` — byte layout was wrong (see Outcome).**

## Acceptance
All checklist items pass on real hardware. If the report byte layout differs from the
spec's assumption, update `src/protocol.rs` and add a regression test. ✅ Met.

## Outcome (2026-06-30)
Bring-up succeeded on real hardware. USB open + interface claim, `SendInput` injection,
and config hot-reload all confirmed working. **One real bug found and fixed:**

- **Bug:** `ReportParser` read the G-key bitmask from report **bytes 1,2,3**. On real
  hardware bytes 1,2 are the **joystick X/Y axes** (centered at `0x7F`), so at idle the
  parser decoded `0x7F7F` as G1–G7 + G9–G15 "pressed" — a constant cascade of phantom
  keypresses (alt+tab, win+d, ctrl+s…) that stole window focus.
- **Root cause:** verified by logging raw reports and pressing G1→G22 in order. The real
  layout is **byte 3 = G1–G8, byte 4 = G9–G16, byte 5 = G17–G22** (byte 5 bit7 `0x80`
  is a constant flag). Joystick X/Y = bytes 1,2; joystick button = byte 7 bit7.
- **Fix:** `src/protocol.rs` now reads bytes 3,4,5; bit→key map was already correct.
  Tests updated to real captured data + added `idle_report_emits_no_events` regression.
  29 tests pass.
- **Diagnostics added:** per-key `debug!` in `dispatcher`; raw-report `trace!` in `usb`.

### Corrected report layout (verified)
| Byte | Contents |
|------|----------|
| 0 | report id (`0x01`) |
| 1 | joystick X (center `0x7F`) |
| 2 | joystick Y (center `0x7F`) |
| 3 | G1–G8 (bits 0–7) |
| 4 | G9–G16 (bits 0–7) |
| 5 | G17–G22 (bits 0–5); bit7 = constant flag |
| 6 | unused (`0x00`) |
| 7 | joystick button (bit7 `0x80`) + M-keys (TBD) |

> Note: bytes 6/7 and the M-key/joystick-button bits still need confirming for v0.2
> (joystick→WASD, M-key profiles). Byte 7's `0x80` toggled during the sweep — likely the
> joystick click; decode it when wiring the joystick.

## Notes
- Original assumed layout (from g13d community RE) was **wrong for this unit**: it put the
  G-key bitmask in bytes 1–3. Corrected above.
- The example `config.toml` binds **G8 → ctrl+alt+delete**, which Windows treats as a Secure
  Attention Sequence and blocks from `SendInput` — a poor demo default; consider changing it.
- Reverting WinUSB -> HID (to use GHub again) is documented at the bottom of `docs/zadig-setup.md`.
