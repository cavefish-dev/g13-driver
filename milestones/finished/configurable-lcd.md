# Configurable LCD

- **Status:** finished
- **Target:** v0.2
- **Updated:** 2026-07-17

## Goal
Make the LCD's three lines configurable via `[lcd]`, and enrich line 3 to track
joystick directions (with sub-project A's labels) and a "while held" mode.

## Tasks
- [x] `[lcd]` config: `LcdConfig` parse + defaults + setters + `persist_lcd`.
- [x] `ActivityTracker` (replaces `capture`): discrete keys + joystick directions; held vs last.
- [x] `render(model, cfg)`: per-line config (name/version, clock, mode label/icon/off; filename/display + ASCII sanitize; line-3 mapping/label flags).
- [x] Clock via `GetLocalTime` (no new dep).
- [x] Wire tracker + config + clock into the poller and both runtimes; GUI preview reads them.
- [x] LCD-tab GUI controls (dropdowns + checkboxes), persist to `[lcd]`.

## Acceptance
Each knob changes the physical LCD + preview live; joystick direction labels show on
line 3 in both trigger modes; clock ticks; unicode display names sanitize to `*`.

## Hardware smoke test (manual) — PASSED 2026-07-17 (visual GUI+preview pass; user approved)
- [ ] Each [lcd] knob changes the physical LCD + preview live (line1 name/version, clock on/off, mode label/icon/off; line2 filename/display; line3 last/held, mapping/label).
- [ ] Joystick direction labels appear on line 3 (both "last" and "held" triggers).
- [ ] Clock shows current HH:MM and ticks.
- [ ] A display name with non-ASCII characters renders '*' for those chars.
- [ ] Existing config with no [lcd] section still works (defaults).

## Notes
- Design: `docs/superpowers/specs/2026-07-17-configurable-lcd-design.md`.
- Sub-project B of two; A (joystick labels + repeat) is merged and provides the labels.

## Follow-up requests (during smoke)
- Center the line-1 clock (currently right-clustered).
- Light the MR LED while in dry-run mode (tracked separately).
