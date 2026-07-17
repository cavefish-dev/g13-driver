# Pre-release polish

- **Status:** finished
- **Target:** v0.2 (pre-release)
- **Updated:** 2026-07-17

## Goal
Two small device-usability enhancements before cutting the release.

## Tasks
- [x] MR key toggles dry-run/active in the GUI.
- [x] Headless gains a dry-run/active mode; MR toggles it (device-controllable service).
- [x] LCD shows the active profile filename without the `.toml` extension.

## Acceptance
Pressing MR flips injection on/off (mode box updates) in both GUI and headless;
the LCD profile line reads e.g. `basic`, not `basic.toml`.

## Hardware smoke test (manual) — PASSED 2026-07-17
- [x] GUI: press MR on the device → injection stops (Dry-run) / starts (Active),
      and the LCD mode box + UI reflect it.
- [x] LCD profile line shows the filename without `.toml` (e.g. `basic`).
- [~] Headless (`--headless`) MR toggle: same reviewed gating pattern as the GUI
      (verified), not separately re-exercised this session.
- [~] M1/M2/M3 profile switching: pre-existing behavior, unchanged by this work.

## Notes
- Design: `docs/superpowers/specs/2026-07-17-mr-toggle-lcd-stem-design.md`.
- GUI smoke passed: MR flipped ACTIVE↔DRY-RUN (mode box + injection gating) and
  the LCD showed the bare filename.
