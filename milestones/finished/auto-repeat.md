# Auto-repeat (typematic) for held keys

- **Status:** finished
- **Date:** 2026-07-09

## Outcome
Hardware-verified. Held hold-means-hold bindings auto-repeat like a physical keyboard. Spec:
`docs/superpowers/specs/2026-07-09-auto-repeat-design.md`; plan:
`docs/superpowers/plans/2026-07-09-auto-repeat.md`.
- Windows does not auto-repeat *injected* keys (typematic is tied to the physical device at the
  raw-input layer; `SendInput` sits above it), so the driver re-injects. A ~15ms `tick` from the
  consumer loops (`runtime::run_headless` + `monitor::consumer_loop`) calls `Dispatcher::tick`,
  which re-fires held, repeat-enabled keys after an initial delay at a steady interval.
- Global timing in the manifest `[autorepeat]` (`delay_ms`/`interval_ms`, defaults 400/40,
  interval clamped to >=1); per-binding opt-in via each profile's `[repeat]` table, edited with a
  checkbox in the Bindings tab. Repeat re-fires the combo's **key only** (modifiers stay held);
  modifier-only and joystick never repeat; media keys still tap.
- No new thread/locks — repeat state lives in `held_keys` (now a `HeldKey` with a schedule), so
  release / Dry-run / disconnect / shutdown stop repeats for free.
- The schedule anchors on the first tick after press (deterministic, injected-time tests — no
  wall-clock race). Verified: `G1 = a` (repeat on) repeats while held; `G2 = b` (repeat off)
  holds but types once; release stops cleanly; `[autorepeat]` timing hot-reloads.

## Follow-ups
- GUI editor for the global `[autorepeat]` timing (planned Settings tab; edited in config.toml for now).
- Per-binding timing overrides (timing is global only).
- Comment-preserving / deterministic-order profile saves (a GUI save still rewrites the file,
  stripping comments and reordering keys — shared with the thumb-buttons follow-up).
