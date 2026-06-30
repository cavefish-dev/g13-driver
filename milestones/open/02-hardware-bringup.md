# Hardware bring-up & smoke test

- **Status:** open
- **Target:** v0.1 (verification)
- **Updated:** 2026-06-30

## Goal
Verify the MVP actually works against a physical G13. This is the one piece the unit
tests can't cover (USB read + `SendInput` injection). **Do this before any new feature.**

## Tasks
- [ ] Connect the G13 via USB.
- [ ] Run Zadig once to replace the G13's HID driver with WinUSB (`docs/zadig-setup.md`).
- [ ] `RUST_LOG=debug cargo run` from a dir containing `config.toml`; confirm
      `g13-driver running` and no `G13 not found` / claim-interface errors.
- [ ] Open Notepad, type text, press **G1** -> text copied (Ctrl+C); **G2** -> pastes (Ctrl+V).
- [ ] Confirm no `SendInput returned 0` warnings in the log.
- [ ] Edit `config.toml` (e.g. G5 `"f5"` -> `"ctrl+p"`), save -> log shows `config reloaded`;
      press G5 -> Print dialog opens.
- [ ] Verify the G-key bit mapping matches the real device (esp. G8/G9 byte boundary,
      G22 high bit). Fix `ReportParser` / report layout if any key is off.

## Acceptance
All checklist items pass on real hardware. If the report byte layout differs from the
spec's assumption, update `src/protocol.rs` and add a regression test.

## Notes
- Report layout assumed (from g13d community RE): byte 0 = report id, bytes 1–3 = G1–G22
  bitmask, byte 4 = M-keys + joystick button, bytes 5–6 = joystick X/Y, byte 7 unused.
- Reverting WinUSB -> HID (to use GHub again) is documented at the bottom of `docs/zadig-setup.md`.
