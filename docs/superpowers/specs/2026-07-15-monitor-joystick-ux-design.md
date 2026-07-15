# Monitor layout & programmable joystick — design

- **Status:** approved (design)
- **Date:** 2026-07-15
- **Scope:** Make the Monitor tab mirror the physical G13 (M-keys on top, thumb buttons beside the
  joystick) and make the joystick programmable from the GUI. Simplify the joystick model: per-profile
  directions only, a single global deadzone on the Settings tab, no mouse mode.

## Motivation

The Monitor doesn't show the thumb buttons at all, puts the M-keys at the bottom (they're at the top
on the device), and the joystick panel shows a fake `wasd` when a profile has no `[joystick]`. The
joystick's directions/deadzone are also only hand-editable in the TOML. This reworks the Monitor to
match the hardware and adds joystick editing to the GUI.

## Decisions (from brainstorming)

- **No mouse mode** — WASD (stick direction → key combo) is the only behavior. `mode` is removed
  from the model.
- **Deadzone is global**, on the Settings tab (like the existing global `[autorepeat]`), not
  per-profile.
- **Per-profile `[joystick]` = the four directions only.**
- **Joystick directions are edited on the Bindings tab**, validated like key combos; empty = unmapped.
- **Monitor:** M-keys row on top; thumb buttons (Btn1/Btn2/Stick) as cells to the left of the
  joystick; joystick shows the profile's real bindings (or "(unset)"), deadzone circle from the
  global value.

## Joystick model change (schema)

- **`JoystickConfig`** (config.rs) drops `mode` and `deadzone`; keeps the four `Option<String>`
  directions (`up`/`down`/`left`/`right`).
- **`RawJoystick`** still *parses* `mode` and `deadzone` (serde-default) so existing profiles load
  without error, but `from_raw`/`parse_joystick` ignore them. On the next GUI save those lines drop
  out (`to_toml` writes `[joystick]` with only the non-empty directions; omits the table when all
  four are empty).
- **Global deadzone:** `config.toml` (the manifest) gains a `[joystick] deadzone = <0..=127>` table,
  parsed onto `ProfileSet` (default 30; out-of-range clamped to ≤127) like the existing global
  `[autorepeat]`. `ProfileSet` exposes `joystick_deadzone() -> u8` and
  `persist_joystick_deadzone(u8) -> Result<()>` (format-preserving `toml_edit`, mirroring
  `persist_start_active`).
- **Consumers read the global deadzone:** `dispatcher::handle_joystick` (and `joystick.rs`) take the
  deadzone from the shared `ProfileSet` rather than the per-profile config, and no longer filter on
  `mode` — any profile with directions is active. The Monitor's deadzone circle uses the global value.
- **Disable is implicit:** an empty direction combo is unmapped; all-empty ⇒ no `[joystick]` ⇒ stick
  inert (as today for a profile with no joystick).

## Settings tab: global deadzone

- A **Joystick deadzone** slider (0–127) with the numeric value beside it, reading/writing the global
  deadzone via `persist_joystick_deadzone` (updates the in-memory `ProfileSet` and `config.toml`).
- Hint: *"Distance the stick must move from center before a direction fires (applies to all
  profiles)."*
- Takes effect immediately — the dispatcher and the Monitor read the global deadzone live from the
  shared `ProfileSet`. Best-effort persistence: a write failure logs + shows a brief status, no crash.
- Placed alongside the existing Settings controls (version / update / autostart / active mode).

## Bindings tab: joystick direction editor

- A **Joystick** section below the key rows + thumb section: four rows (**Up / Down / Left / Right**),
  each a combo text field validated exactly like a key binding (`ok`/`bad`/`—` via `combo_valid`).
  Empty = that direction unmapped.
- A `joy_edits: [String; 4]` buffer on `MonitorApp` (up/down/left/right order), reloaded from the
  active profile's `[joystick]` when the binding buffers reload, saved on **Save**.
- **Save** extends `save_active_bindings` to also persist the joystick: build the four directions
  into the profile's `JoystickConfig` (all-empty ⇒ no `[joystick]`). Editing a direction on a GitHub
  profile flips `modified`, same as keys/labels.
- No deadzone or mode control here (deadzone is on Settings; mode is gone). The Bindings help text
  drops the stale "stick lives in `[joystick]`/hand-edit" note.

## Monitor layout rework (`render_monitor`)

```
   M1  M2  M3  MR                 ← M-keys row, moved to TOP (active highlighted, read-only)

   ┌────┐┌────┐┌────┐ …           ← G-key grid (key · combo · label), unchanged
   ─────────────────────
   ┌────┐    ┌───────────┐
   │Btn1│    │  joystick │        ← thumb cells (LEFT)  │  joystick viz (RIGHT)
   │Btn2│    │    ● dot  │
   │Stick│   └───────────┘
   └────┘    ↑up ↓down ←left →right
```

- **M-keys row → top** of the view (same indicator, relocated above the grid).
- **Thumb buttons** (`Btn1`, `Btn2`, `Stick`) render as cells in the same key · combo · label format
  as G-keys, in a column to the **left** of the joystick panel.
- **Joystick shows real bindings:** the direction labels read the profile's actual `up/down/left/
  right` (dimmed **"(unset)"** when empty — replacing the fake `wasd` fallback at the current
  `unwrap_or((30,"w","s","a","d"))`); the deadzone circle uses the global deadzone; the live stick
  dot is unchanged.

## Shipped content

- `config.toml` gains `[joystick] deadzone = 30`.
- Bundled/catalog profiles with a `[joystick]` (basic, gaming) are cleaned to directions-only (drop
  the now-ignored `mode`/`deadzone` lines).

## Error handling

Global-deadzone parse clamps to 0–127 (default 30 when absent/invalid); persist failures log +
surface a status, never crash; joystick save is best-effort within the existing `save_active_bindings`
`Result`; no `panic!`/`unwrap()`/`expect()` on profile data (lock-poison `.unwrap()` excepted).

## Testing

- **Unit (TDD, pure logic):**
  - `JoystickConfig` parses directions-only; a legacy `[joystick]` with `mode`/`deadzone` still loads
    (those ignored); `to_toml` writes `[joystick]` with only non-empty directions and omits it when
    all empty.
  - Global `[joystick] deadzone` parse: present / absent→30 / >127 clamped; `persist_joystick_deadzone`
    is format-preserving + reloads.
  - `save_active_bindings` persists joystick directions (all-empty ⇒ no `[joystick]`) and flips
    `modified` on a GitHub profile.
- **Manual-verify (GUI, documented exception):** Monitor shows M-keys on top, thumb cells left of the
  joystick, and real/"(unset)" joystick bindings; the Settings deadzone slider persists and the
  Monitor circle reflects it; the Bindings joystick direction fields edit + save.

## Out of scope (follow-ups)

- Mouse mode for the stick (removed; a future feature would reintroduce a mode).
- Per-profile deadzone (now global by decision).
- Labels for joystick directions (directions are self-describing).
- Editing M-key slot assignment from the Monitor (that's the Profiles tab).
