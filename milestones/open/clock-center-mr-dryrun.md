# Centered clock + MR dry-run LED

- **Status:** open
- **Target:** v0.2
- **Updated:** 2026-07-17

## Goal
Two device-UX tweaks: center the line-1 clock, and light the MR LED during dry-run
(gated by the M-key indicator toggle).

## Tasks
- [ ] `led::resolve` + `desired_led_state` gain `dry_run`; MR bit (8) set in dry-run when `mkey_indicator`.
- [ ] `led::spawn_poller` gains `dry_run`; both runtimes pass their flag.
- [ ] Render the line-1 clock centered.

## Acceptance
Clock is centered on the LCD; MR LED lights in dry-run and goes dark in active (and stays
dark if the M-key indicator is off).

## Notes
- Design: `docs/superpowers/specs/2026-07-17-clock-center-mr-dryrun-design.md`.
- Follow-up from the configurable-LCD smoke test.
