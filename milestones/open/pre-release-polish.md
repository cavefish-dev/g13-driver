# Pre-release polish

- **Status:** open
- **Target:** v0.2 (pre-release)
- **Updated:** 2026-07-17

## Goal
Two small device-usability enhancements before cutting the release.

## Tasks
- [ ] MR key toggles dry-run/active in the GUI.
- [ ] Headless gains a dry-run/active mode; MR toggles it (device-controllable service).
- [ ] LCD shows the active profile filename without the `.toml` extension.

## Acceptance
Pressing MR flips injection on/off (mode box updates) in both GUI and headless;
the LCD profile line reads e.g. `basic`, not `basic.toml`.

## Notes
- Design: `docs/superpowers/specs/2026-07-17-mr-toggle-lcd-stem-design.md`.
- Needs a hardware smoke test (MR press) before the release.
