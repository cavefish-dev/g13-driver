# Minor improvements

- **Status:** finished
- **Target:** v0.2
- **Updated:** 2026-07-17

## Goal
Two no-behavior-change cleanups flagged during code review.

## Tasks
- [x] `active_name_stem` uses `strip_suffix(".toml")` (strip one, not all trailing occurrences).
- [x] `handle_joystick` folds the per-direction repeat-flag read into its up-front `(cfg, deadzone, autorepeat)` snapshot — one lock acquisition instead of a second `profiles.read()` per KeyDown (removes a theoretical TOCTOU).

## Notes
- Design/plan: `docs/superpowers/plans/2026-07-17-minor-improvements.md`.
- Follow-ups from the joystick-labels-repeat and configurable-LCD reviews. Existing
  tests (`active_name_stem_strips_toml_extension`, `joystick_repeat_refires_on_tick`) cover both.
