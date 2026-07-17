# Centered clock + MR dry-run LED

- **Status:** finished
- **Target:** v0.2
- **Updated:** 2026-07-17

## Goal
Two device-UX tweaks: center the line-1 clock, and light the MR LED during dry-run
(gated by the M-key indicator toggle).

## Tasks
- [x] `led::resolve` + `desired_led_state` gain `dry_run`; MR bit (8) set in dry-run when `mkey_indicator`.
- [x] `led::spawn_poller` gains `dry_run`; both runtimes pass their flag.
- [x] Render the line-1 clock centered.

## Acceptance
Clock is centered on the LCD; MR LED lights in dry-run and goes dark in active (and stays
dark if the M-key indicator is off).

## Hardware smoke test (manual) — PASSED 2026-07-17
- [x] The line-1 clock renders centered on the physical LCD.
- [x] Toggling to dry-run (MR key) lights the MR LED; returning to active turns it off.
- [x] With the M-key indicator turned off (Settings), MR stays dark even in dry-run.

## Notes
- Design: `docs/superpowers/specs/2026-07-17-clock-center-mr-dryrun-design.md`.
- Follow-up from the configurable-LCD smoke test.
