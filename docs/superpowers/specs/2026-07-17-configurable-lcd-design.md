# Configurable LCD — Design

- **Date:** 2026-07-17
- **Milestone:** `milestones/open/configurable-lcd.md` (new)
- **Status:** approved, ready for implementation plan

Sub-project **B** (of two; A = joystick labels + repeat, merged). Makes each of the
LCD's three content lines configurable, and enriches line 3 to track joystick
directions (using A's labels) with a "while held" option.

## Config — `[lcd]` in `config.toml` (global, mirrors `[backlight]`)

```toml
[lcd]
line1_left    = "name"      # "name" (G13 Driver) | "version" (v{VERSION})
line1_clock   = false       # show an HH:MM clock
line1_mode    = "label"     # "label" (box + ACTIVE/DRY-RUN) | "icon" (box only) | "off"
line2_source  = "filename"  # "filename" (stem) | "display" ([meta] name, fallback stem)
line3_trigger = "last"      # "last" (persist last pressed) | "held" (while held)
line3_mapping = true        # show the combo/key
line3_label   = true        # show the mapping's label
```

- `LcdConfig { line1_left: Line1Left, line1_clock: bool, line1_mode: ModeDisplay,
  line2_source: Line2Source, line3_trigger: Line3Trigger, line3_mapping: bool,
  line3_label: bool }` with enums parsed from the strings above; unknown enum value
  → warn and use the default for that field (never panic), mirroring `[backlight]`.
- A missing `[lcd]` section → all defaults (the values shown above).
- `ProfileSet` gains `lcd_config()`, per-field setters, and `persist_lcd()`
  (format-preserving, same shape as `persist_backlight`).

## Activity tracker (replaces `capture`)

The stateless `capture(event, profiles, cell)` becomes a stateful
`ActivityTracker`, shared behind `Arc<Mutex<ActivityTracker>>` (event loop writes,
poller + GUI preview read).

- State: a `JoystickMapper` (reports direction, from sub-project A), an ordered
  held list keyed by `HeldId { Key(G13Key), Dir(JoystickDir) }` → `LastAction`, and a
  `last: Option<LastAction>`.
- `on_event(&mut self, event: &G13Event, profiles: &Arc<RwLock<ProfileSet>>)`:
  - `KeyDown(key)`: resolve `{ button: "{key:?}", combo: get_binding(key),
    label: label(key) }` from the active profile; upsert into held as most-recent; set `last`.
  - `KeyUp(key)`: remove `HeldId::Key(key)` from held.
  - `JoystickMove{x,y}`: snapshot the active profile's joystick config + global
    deadzone; run the mapper. For each `HoldAction::KeyDown{dir,key}`: resolve
    `{ button: dir name (Up/Down/Left/Right), combo: Some(key), label: joystick_label(dir) }`;
    upsert as most-recent; set `last`. For each `KeyUp{dir,key}`: remove `HeldId::Dir(dir)`.
  - `MKeyDown/MKeyUp`, `KeyUp` of unheld keys: no-op / harmless.
  - On `MKeyDown` (profile switch): clear held + reset the mapper (a new profile may
    rebind the stick), so stale joystick holds don't linger.
- `current(&self, trigger: Line3Trigger) -> Option<LastAction>`:
  - `Last` → `self.last.clone()`.
  - `Held` → the most-recently-added still-held entry (last of the ordered list), else `None`.

The `Arc<Mutex<Option<LastAction>>>` cell used today is replaced by
`Arc<Mutex<ActivityTracker>>`; the poller and the GUI preview read `current(trigger)`.

## Rendering (`render(model: &LcdModel, cfg: &LcdConfig)`)

`LcdModel` carries raw resolved state:
`{ mode: Mode, slot: MKey, filename: Option<String>, display_name: Option<String>,
last: Option<LastAction>, clock: Option<String> }` (the poller precomputes `last` per
`line3_trigger` and `clock` per `line1_clock`).

- **Line 1 (y0):**
  - Left: `"G13 Driver"` (`Line1Left::Name`) or `"v{G13_VERSION}"` (`Version`).
  - Right cluster: optional clock (`HH:MM`) then the mode per `line1_mode`:
    `Label` = box (filled=Active / hollow=Dry-run) + `ACTIVE`/`DRY-RUN`; `Icon` = box
    only; `Off` = nothing. Right-clustered so both fit in 160 px (≈26 chars); when the
    left text + right cluster would overflow, the left text is truncated.
- **Divider (y9).**
- **Line 2 (slot + name, name at 2×, y12/16):** slot label + (`filename` stem or
  `display_name`) per `line2_source`; `display_name` falls back to the filename stem
  when the `[meta]` name is unset. **Sanitize:** every char outside `0x20..=0x7E` is
  replaced with `*` before drawing (the font is ASCII-only). Truncated to fit.
- **Line 3 (y32):** from `model.last`. Always show the button/direction name; append
  the combo only if `line3_mapping` (else omit; an unbound key shows `(unbound)` only
  when mapping is on); append the label only if `line3_label` and it exists.

## Clock

`HH:MM` local time via `windows_sys::Win32::System::SystemInformation::GetLocalTime`
(`GetLocalTime` fills a `SYSTEMTIME` with local `wHour`/`wMinute`) — **no new crate**
(app is Windows-only). The poller computes the string only when `line1_clock` is on;
the ~150 ms poll refresh keeps it current to the minute. A small `#[cfg(windows)]`
helper `local_hh_mm() -> String`; non-Windows returns `""` (the crate is Windows-only
anyway, but keep the platform-isolation convention).

## GUI (LCD tab)

Above the live preview, add controls persisting to `[lcd]` (mirroring the backlight
controls' persist pattern — in-memory setter + `persist_lcd()`, warn on error):
- Dropdowns (`egui::ComboBox`): `line1_left`, `line1_mode`, `line2_source`, `line3_trigger`.
- Checkboxes: `line1_clock`, `line3_mapping`, `line3_label`.
The preview rebuilds the `LcdModel` from live state + `lcd_config()` + the tracker each
frame, so edits show immediately.

## Testing

**Unit (TDD):**
- Config: `[lcd]` parse (each field), missing-section defaults, bad enum value → default
  + warn, persist round-trip preserving other keys.
- `ActivityTracker`: KeyDown/KeyUp held + last; joystick press/release via the mapper
  (direction name + bound key + `joystick_label`); `Held` returns most-recent-still-held
  and `None` when nothing held; `Last` persists after release; `MKeyDown` clears held.
- `render`: line-1 name-vs-version, clock present/absent, mode label/icon/off; line-2
  filename-vs-display + fallback + non-ASCII→`*` sanitize; line-3 mapping/label flags,
  unbound handling; truncation. (Clock string is passed in, so `render` stays pure and
  time-independent.)

**Manual smoke:** each knob changes the physical LCD + preview; joystick direction
labels appear on line 3 in both trigger modes; clock ticks; display-name/unicode
sanitizes to `*`.

## Out of scope

- CPU/RAM/system metrics.
- Per-profile LCD config (global only).
- Any knob beyond the seven listed; multiple clock formats/timezones.
- Non-Windows clock (stubbed).
