# Pre-release polish

- **Status:** ongoing
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

## Hardware smoke test (manual)
- [ ] GUI: press MR on the device → injection stops (Dry-run) / starts (Active),
      and the LCD mode box + tray/UI reflect it.
- [ ] Headless (`--headless`): press MR → keystrokes stop/start injecting; the
      LCD mode box flips ACTIVE↔DRY-RUN.
- [ ] M1/M2/M3 still switch profiles in both runtimes.
- [ ] LCD profile line shows the filename without `.toml` (e.g. `basic`).

## Notes
- Design: `docs/superpowers/specs/2026-07-17-mr-toggle-lcd-stem-design.md`.
- Needs a hardware smoke test (MR press) before the release.
