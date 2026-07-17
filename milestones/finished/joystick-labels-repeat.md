# Joystick labels + repeat

- **Status:** finished
- **Target:** v0.2
- **Updated:** 2026-07-17

## Goal
Make joystick directions first-class bindings: each gains an optional label and an
auto-repeat flag, matching G-keys. Prerequisite for the configurable-LCD work (B),
which shows joystick labels on the display.

## Tasks
- [x] Schema: per-direction label + repeat (`[joystick.labels]` / `[joystick.repeat]`), parse + persist, backward-compatible.
- [x] `JoystickMapper` reports which direction fired (direction-annotated actions).
- [x] Dispatcher: repeat-enabled held directions auto-repeat via `tick()` (global interval); stop on release.
- [x] GUI Bindings tab: label field + repeat checkbox per direction.

## Acceptance
A joystick direction with `repeat = true` auto-repeats its key while held; labels and
repeat flags edit in the Bindings tab and persist. Existing profiles keep working.

## Hardware smoke test (manual) — PASSED 2026-07-17
- [x] A joystick direction with `repeat = true` auto-repeats its key while held;
      `repeat = false` holds without repeating.
- [x] Labels + repeat flags set in the Bindings tab save and survive a reload.
- [x] An existing profile with a plain `[joystick]` still loads and works unchanged.

## Follow-up
- User feedback: the joystick binding-row UI is preferred over the G-key row UI;
  unify all Bindings-tab mappings to one style (tracked separately).

## Notes
- Design: `docs/superpowers/specs/2026-07-17-joystick-labels-repeat-design.md`.
- Sub-project A of two; B = configurable LCD (consumes these labels).
