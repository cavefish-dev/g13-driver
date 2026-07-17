# Joystick Labels + Repeat — Design

- **Date:** 2026-07-17
- **Milestone:** `milestones/open/joystick-labels-repeat.md` (new)
- **Status:** approved, ready for implementation plan

Sub-project **A** of a two-part effort. Makes joystick directions first-class
bindings — each gains an optional **label** and an **auto-repeat flag**, matching
what G-keys already have (`[labels]` / `[repeat]`). Sub-project **B** (configurable
LCD) will consume the joystick labels; it is a separate spec.

## Background

- A profile's joystick lives in `JoystickConfig { up, down, left, right: Option<String> }`,
  parsed from the profile's `[joystick]` section. Only the key per direction is stored.
- The dispatcher's `handle_joystick` calls `JoystickMapper::update(x, y, cfg, deadzone)`,
  which returns `HoldAction::KeyDown(key)/KeyUp(key)` (key string only) and holds the
  key via the injector. **Joystick keys are not in `held_keys`, so they never
  auto-repeat** — `tick()` only repeats discrete `held_keys`.
- G-keys carry `[labels]` (`HashMap<G13Key, String>`) and `[repeat]`
  (`HashMap<G13Key, bool>`); `Profile::label(key)` / `repeats(key)` read them.

## Schema (backward-compatible)

Add optional nested tables under the profile's `[joystick]`:

```toml
[joystick]
up = "w"
down = "s"
left = "a"
right = "d"

[joystick.labels]      # optional
up   = "Forward"
left = "Strafe L"

[joystick.repeat]      # optional
down = true
```

- Existing profiles (no `[joystick.labels]`/`[joystick.repeat]`) parse unchanged
  (empty maps → no labels, repeat off).
- Direction keys are validated against `up`/`down`/`left`/`right`; an unknown key is
  a load error (mirrors the `[labels]`/`[repeat]` unknown-key errors for G-keys).
- `JoystickConfig` gains `labels`/`repeat` storage plus accessors
  `label(dir) -> Option<&str>` and `repeats(dir) -> bool`, where
  `dir: JoystickDir { Up, Down, Left, Right }`.

## Auto-repeat behavior

A held direction with `repeat = true` re-fires its key at the **global
`[autorepeat]` delay/interval** — the same mechanism and timing G-keys use.

- `JoystickMapper::update` (and `release_all`) start returning **direction-annotated**
  actions (which `JoystickDir` fired, plus the key), so the dispatcher can look up
  the direction's repeat flag.
- The dispatcher registers a repeat-enabled held direction into the auto-repeat loop
  and `tick()` re-fires it at the global interval; the entry is dropped on the
  direction's release, on `release_joystick` (profile switch), and on `release_held`
  (dry-run / disconnect / shutdown) — nothing sticks.
- `repeat = false` (default) keeps today's behavior exactly (hold the key, no re-fire).

## GUI — Bindings tab joystick editor

The joystick editor's four direction rows each gain a **label text field** and a
**repeat checkbox**, matching the existing G-key binding-row shape. Save writes
`[joystick.labels]` / `[joystick.repeat]` format-preserving (via `toml_edit`,
mirroring how G-key labels/repeat persist), and clears a key's entry when emptied.

## Testing

**Unit (TDD):**
- Config: `[joystick.labels]`/`[joystick.repeat]` parse into `JoystickConfig`;
  backward-compat when absent; unknown direction key → load error; persist round-trip
  preserving other manifest keys.
- `JoystickMapper`: `update`/`release_all` report the correct `JoystickDir` for each
  action (existing key-transition behavior unchanged).
- Dispatcher: a repeat-enabled held direction re-fires on `tick()` at the interval; a
  non-repeat direction does not; release (center / profile switch / `release_held`)
  removes the repeat entry so it stops.

**Manual smoke:**
- A direction with `repeat = true` auto-repeats its key while held; `repeat = false`
  holds without repeating.
- Labels + repeat flags edited in the Bindings tab save and reload.

## Out of scope

- LCD showing joystick labels (sub-project B).
- Per-direction custom repeat timing (uses the global `[autorepeat]`).
- Any change to G-key label/repeat behavior or to non-joystick dispatch.
