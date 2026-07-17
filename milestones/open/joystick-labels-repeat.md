# Joystick labels + repeat

- **Status:** open
- **Target:** v0.2
- **Updated:** 2026-07-17

## Goal
Make joystick directions first-class bindings: each gains an optional label and an
auto-repeat flag, matching G-keys. Prerequisite for the configurable-LCD work (B),
which shows joystick labels on the display.

## Tasks
- [ ] Schema: per-direction label + repeat (`[joystick.labels]` / `[joystick.repeat]`), parse + persist, backward-compatible.
- [ ] `JoystickMapper` reports which direction fired (direction-annotated actions).
- [ ] Dispatcher: repeat-enabled held directions auto-repeat via `tick()` (global interval); stop on release.
- [ ] GUI Bindings tab: label field + repeat checkbox per direction.

## Acceptance
A joystick direction with `repeat = true` auto-repeats its key while held; labels and
repeat flags edit in the Bindings tab and persist. Existing profiles keep working.

## Notes
- Design: `docs/superpowers/specs/2026-07-17-joystick-labels-repeat-design.md`.
- Sub-project A of two; B = configurable LCD (consumes these labels).
