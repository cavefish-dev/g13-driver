# Joystick → WASD/movement — design

- **Status:** approved (design)
- **Date:** 2026-06-30
- **Part of:** v0.2 (`milestones/open/v0.2-joystick-mkeys-service.md`), sub-project 1 of 3
- **Scope:** Decode the G13 analog joystick and translate stick direction into held
  keystrokes (WASD-style). Mouse-movement mode is designed into the config seam but
  **not implemented** in this sub-project.

## Context

v0.2 bundles three independent subsystems: (1) joystick→movement, (2) M-key profile
switching, (3) Windows Service. They are being built as separate spec→plan→build cycles.
This is sub-project **1**.

The full G13 report layout was verified on real hardware during the v0.2 bring-up capture
(see `milestones/finished/02-hardware-bringup.md` for the key-byte fix, and below for the
joystick/M-key bytes confirmed afterward):

| Byte | Contents |
|------|----------|
| 0 | report id `0x01` |
| 1 | **joystick X**: `0x00` left · `0x7F` (127) center · `0xFF` right |
| 2 | **joystick Y**: `0x00` up · `0x7F` (127) center · `0xFF` down |
| 3 | G1–G8 (bits 0–7) |
| 4 | G9–G16 (bits 0–7) |
| 5 | G17–G22 (bits 0–5); bit7 = constant flag |
| 6 | M1 = bit5, M2 = bit6, M3 = bit7 |
| 7 | MR = bit0, joystick click = bit3, bit7 = noisy heartbeat (mask) |

This sub-project uses **bytes 1 and 2 only**. Bytes 6/7 (M-keys, click) are decoded in
sub-project 2.

## Goal

Push the stick in a direction → the configured key is held down; return to center →
released. Independent per-axis thresholding yields 8-way movement (diagonals hold two
keys at once). Keys are config-driven (default WASD, but rebindable, e.g. to arrows).

## Approach

**Joystick as a new event type** (chosen over synthetic key events in the parser, and
over a separate parallel path). It fits the existing single-stream pipeline
`UsbReader → ReportParser → Dispatcher → KeyInjector`, keeps the protocol layer a pure
byte→event decoder, and isolates all joystick *policy* in one fully unit-testable
component. The injector gains true key-hold methods, reusable later for v0.3 macros.

## Components & interfaces

### `src/protocol.rs`
- Add `G13Event::JoystickMove { x: u8, y: u8 }`.
- `ReportParser` tracks `prev_x`/`prev_y` (bytes 1, 2). When either changes, push exactly
  one `JoystickMove`. Key decoding unchanged. M-keys/click remain undecoded here.

### `src/joystick.rs` (new) — `JoystickMapper`
- State: `deadzone: u8`, four targets (`up/down/left/right: Option<String>`), current
  per-axis held direction (`x_held: Option<Dir>`, `y_held: Option<Dir>`).
- `fn update(&mut self, x: u8, y: u8) -> Vec<HoldAction>` — **pure**: returns
  `KeyDown(key)` / `KeyUp(key)` transitions from the per-axis threshold model. No OS calls.
- `fn release_all(&mut self) -> Vec<HoldAction>` — emits `KeyUp` for every held key and
  clears state; idempotent (second call emits nothing).
- Center fixed at `127`. A key fires when `value < 127 − deadzone` or `value > 127 + deadzone`.
- `HoldAction` enum: `KeyDown(String)` / `KeyUp(String)`.

### `src/injector/mod.rs` — `KeyInjector` trait
- Add `fn key_down(&self, key: &str) -> Result<()>` and `fn key_up(&self, key: &str) -> Result<()>`.
- Keep `press()`; refactor it to call `key_down` then `key_up`.
- `WindowsInjector` implements both via `SendInput`, setting/clearing `KEYEVENTF_KEYUP`.

### `src/dispatcher.rs`
- Holds a `JoystickMapper`. On `JoystickMove`, call `mapper.update(x, y)` and apply each
  returned `HoldAction` via `key_down`/`key_up`. `KeyDown`/`KeyUp(G13Key)` path unchanged.
- On config hot-reload: call `release_all()` and apply it before swapping the mapper, so a
  held key is lifted before the new mapping takes effect.

### `src/config.rs`
- Add optional `[joystick]` section to `RawConfig`: `mode`, `deadzone`, `up/down/left/right`.
- Absent section → joystick disabled. `Config` exposes a parsed `JoystickConfig`.
- Validation: `deadzone` in `0..=127`; key names resolved against the existing key map;
  `mode` ∈ {`wasd`, `mouse`}. Invalid → load error (bad reload ignored, previous config kept).

### Config schema
```toml
[joystick]
mode = "wasd"      # "wasd" (implemented) | "mouse" (parsed, not yet implemented)
deadzone = 30      # 0-127; distance from center (127) before a key fires
up = "w"
down = "s"
left = "a"
right = "d"
```

## Data flow

```
G13 USB --> UsbReader --> ReportParser --> Dispatcher ----> KeyInjector
                          bytes 1,2 ->     JoystickMove ->   key_down/key_up
                          JoystickMove     JoystickMapper.update(x,y)
                                           -> Vec<HoldAction>
```

## Edge cases & error handling

- **Stuck keys on disconnect/shutdown:** if the stick is deflected when the driver stops
  or USB read errors, held keys would be left down in the OS. The dispatcher calls
  `mapper.release_all()` on shutdown and on USB-read error to lift every held key.
- **Hot-reload while a key is held:** dispatcher calls `release_all()` (and injects the
  releases) before applying the new joystick config.
- **Same key bound twice** (joystick + G-key): inject independently; OS coalesces repeated
  key-downs. Not special-cased.
- **`mode = "mouse"`:** config parses and validates, mapper logs `mouse mode not yet
  implemented` once and treats the stick as inert. No dead behavior, schema ships complete.
- **Injection failure:** `key_down`/`key_up` failures log a warning and continue (same
  policy as `press()`). A dropped key-**up** is the risky case → logged at `warn`.
- **Invalid config** (`deadzone > 127`, unknown key): rejected at load with a clear error.

## Testing plan (TDD)

Pure-logic modules built test-first; `SendInput` verified manually (documented exception).

**`JoystickMapper`:**
- Inside deadzone → no actions.
- Each axis: cross threshold → `KeyDown(dir)`; return to center → `KeyUp(dir)`.
- Diagonal (up-left) → both `KeyDown(up)` and `KeyDown(left)` (8-way model).
- Cross through center left→right → `KeyUp(left)` then `KeyDown(right)` (no stuck key).
- Idempotent: holding within a zone emits no duplicate `KeyDown`.
- `release_all()` with two keys held → `KeyUp` for both; second call emits nothing.
- Unmapped direction (`up = None`) → no action on that axis.

**`ReportParser`:**
- `JoystickMove` emitted only when X or Y changes; centered/idle is idempotent.
- G-key press with stick deflected emits both the key event and the move.

**`config.rs`:**
- `[joystick]` parses into `JoystickConfig`; absent → disabled.
- `deadzone > 127` and unknown key name → load error.
- `mode = "mouse"` parses (validated, marked unimplemented).

**`WindowsInjector`:** no unit tests — verified in the hardware smoke test (stick → character
repeats in Notepad; return-to-center stops them; no stuck keys; hot-reload mid-hold is clean).

## Out of scope (this sub-project)

- Mouse-movement mode (designed into the config seam; implemented later).
- M-key decoding and profile switching (sub-project 2).
- Windows Service (sub-project 3).
- Joystick polling cadence tuning / acceleration curves (only needed for mouse mode).
