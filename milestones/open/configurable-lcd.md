# Configurable LCD

- **Status:** open
- **Target:** v0.2
- **Updated:** 2026-07-17

## Goal
Make the LCD's three lines configurable via `[lcd]`, and enrich line 3 to track
joystick directions (with sub-project A's labels) and a "while held" mode.

## Tasks
- [ ] `[lcd]` config: `LcdConfig` parse + defaults + setters + `persist_lcd`.
- [ ] `ActivityTracker` (replaces `capture`): discrete keys + joystick directions; held vs last.
- [ ] `render(model, cfg)`: per-line config (name/version, clock, mode label/icon/off; filename/display + ASCII sanitize; line-3 mapping/label flags).
- [ ] Clock via `GetLocalTime` (no new dep).
- [ ] Wire tracker + config + clock into the poller and both runtimes; GUI preview reads them.
- [ ] LCD-tab GUI controls (dropdowns + checkboxes), persist to `[lcd]`.

## Acceptance
Each knob changes the physical LCD + preview live; joystick direction labels show on
line 3 in both trigger modes; clock ticks; unicode display names sanitize to `*`.

## Notes
- Design: `docs/superpowers/specs/2026-07-17-configurable-lcd-design.md`.
- Sub-project B of two; A (joystick labels + repeat) is merged and provides the labels.
