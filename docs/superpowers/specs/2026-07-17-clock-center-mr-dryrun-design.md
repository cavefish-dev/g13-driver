# Centered Clock + MR Dry-run LED — Design

- **Date:** 2026-07-17
- **Milestone:** `milestones/open/clock-center-mr-dryrun.md` (new)
- **Status:** approved, ready for implementation plan

Two small device-UX tweaks requested during the configurable-LCD smoke test.

## Tweak 1 — center the line-1 clock

Today the clock is drawn in the line-1 right cluster (just left of the mode). Move
it to the **horizontal center** of line 1.

- In `render` (`src/lcd/mod.rs`), remove the clock from the right-cluster (mode) logic;
  the right cluster becomes mode-only.
- When `cfg.line1_clock` and `model.clock` is `Some`, draw the clock at
  `x = (LCD_W as i32 - text_width(clk, 1)) / 2`, y=0.
- No overlap in practice: a `HH:MM` clock is 30 px, centered at x≈65–95; the left
  title is ≤60 px and the right mode cluster starts ≥110 px, leaving a clear gap.
  (Draws are bounds-safe regardless, so no panic even if a config combination were
  tighter.)

## Tweak 2 — light the MR LED during dry-run

The MR mode key has its own indicator LED (M-key bitmask bit `8`). Light it while the
driver is in **dry-run**, as an "injection paused" signal on the device. **Gated by the
existing `mkey_indicator` toggle** (if M-key LEDs are off, MR stays dark too).

- `led::resolve` gains a `dry_run: bool` param:
  `resolve(active: MKey, dry_run: bool, cfg: &BacklightConfig) -> LedState`. The `mkeys`
  bitmask, when `cfg.mkey_indicator` is true, becomes `active_bit | (if dry_run { 8 } else { 0 })`
  (active_bit = 1/2/4 for M1/M2/M3, 0 for MR). When `mkey_indicator` is false, `mkeys = 0`.
- `ProfileSet::desired_led_state` gains a `dry_run: bool` param and forwards it to `resolve`.
- `led::spawn_poller` gains a `dry_run: Arc<AtomicBool>` param; each tick it reads the flag
  and calls `desired_led_state(dry_run.load(Relaxed))`.
- Both runtimes pass their dry-run flag to `led::spawn_poller`: `run_headless` (the flag
  added by the MR-toggle feature) and the GUI `start_consumer` (`self.dry_run`).

## Testing

- **Unit (TDD):** `resolve` — with `dry_run=true` + `mkey_indicator=true`, `mkeys` has the
  MR bit set (e.g. active M1 → `1 | 8 = 9`); with `dry_run=true` + `mkey_indicator=false`,
  `mkeys = 0` (gated); `dry_run=false` unchanged. Update existing `resolve`/`desired_led_state`
  tests to the new signatures.
- **Render:** clock, when enabled, draws centered (a lit pixel appears in the central
  x-band and none in the far-right cluster position it used to occupy). Existing render
  band tests still pass (ASCII, same title band).
- **Manual smoke:** the clock is centered on the physical LCD; toggling dry-run (MR key)
  lights/extinguishes the MR LED; turning off the M-key indicator keeps MR dark in dry-run.

## Out of scope

- A separate config knob for the MR dry-run indicator (it's gated by the existing
  `mkey_indicator`).
- Any change to M1/M2/M3 indicator behavior or the backlight color path.
