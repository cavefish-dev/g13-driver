# Monitor layout & programmable joystick

- **Status:** finished — GUI smoke-tested (2026-07-16)
- **Date:** 2026-07-16

## Outcome
The Monitor mirrors the physical G13 and the joystick is programmable from the GUI. Spec:
`docs/superpowers/specs/2026-07-15-monitor-joystick-ux-design.md`; plan:
`docs/superpowers/plans/2026-07-15-monitor-joystick-ux.md`.

- **Joystick model simplified:** per-profile `[joystick]` is now the four directions only. `mode`
  (mouse) is removed — WASD (stick direction → key combo) is the only behavior. `JoystickConfig` =
  `{ up, down, left, right: Option<String> }`. Legacy profiles with `mode`/`deadzone` still load
  (those fields parsed but ignored; dropped on next save). `to_toml` writes directions-only and
  omits `[joystick]` when all four are empty.
- **Global deadzone:** a single manifest `[joystick] deadzone` on `ProfileSet` (default **50**,
  clamp ≤127), edited on the **Settings** tab. The dispatcher, `joystick.rs`, and the Monitor all
  read the global value. `persist_joystick_deadzone` is format-preserving.
- **Programmable joystick:** the **Bindings** tab has a Joystick section (Up/Down/Left/Right combo
  fields, validated like key combos); `save_active_bindings` grew a joystick arg. Empty direction =
  unmapped; all-empty ⇒ no `[joystick]`. `joy_edits` reloads from the active profile so a normal
  edit-save round-trips.
- **Monitor rework:** M-keys row on **top** (above the grid, matching the device); thumb buttons
  (Btn1/Btn2/Stick) render as cells (key · combo · label) to the **left** of the joystick; the
  joystick shows the profile's real directions (or dimmed **"(unset)"**) instead of the old fake
  `wasd`, with the deadzone circle from the global value.
- **Shipped content:** `config.toml` global `[joystick] deadzone = 50`; bundled/catalog profiles
  (basic, gaming) cleaned to directions-only.

Built via subagent-driven-development (7 tasks), 173 unit tests. Per-task reviews (Task 2's 5-file
type ripple got an adversarial review confirming total removal + dispatcher lock safety) + a final
whole-branch review (opus): MERGE, no Critical/Important — no stale mode/deadzone, joystick
round-trips, dispatcher lock-safe.

## Smoke test — PASSED 2026-07-16
Verified live: Monitor shows M-keys on top, thumb cells left of the joystick, and real/"(unset)"
joystick bindings; the Settings deadzone slider persists and updates the Monitor circle; the Bindings
joystick fields edit + save. Default deadzone raised to 50 afterward (a wide value that avoids
unintended triggers for new users) — code default + `config.toml` + test updated.

## Follow-ups (deferred)
- Mouse mode for the stick (removed here; a future feature reintroduces a mode).
- Per-profile deadzone (now global by decision).
- Labels for joystick directions (directions are self-describing).
- Minor (final review): pre-existing `render_monitor` read-guard-across-panel idiom; a hardcoded
  thumb-column centering fudge (cosmetic).
