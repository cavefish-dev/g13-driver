# Thumb buttons (near-joystick) — design

- **Status:** approved (design)
- **Date:** 2026-07-05
- **Scope:** Decode the three currently-unbound thumb inputs — the two buttons next to the
  joystick and the joystick click — and make them **first-class bindable buttons** (like
  G-keys), shown in the GUI Monitor and editable in the Bindings tab. Reuses the whole
  binding / hold-means-hold / editor / hot-reload stack.

## Motivation

The GUI Monitor doesn't show (and you can't bind) the two buttons next to the joystick or the
joystick click. A capture identified their bits; making them ordinary `G13Key`s lets the
existing machinery pick them up with almost no new code.

## Report layout (hardware-verified this session)

Byte 7 holds the thumb inputs:

| Input | Byte 7 bit |
|---|---|
| MR (already an M-key) | bit 0 (`0x01`) |
| **Btn1** (side button 1) | **bit 1 (`0x02`)** |
| **Btn2** (side button 2) | **bit 2 (`0x04`)** |
| **Stick** (joystick click) | **bit 3 (`0x08`)** |
| heartbeat / noise | bit 7 (`0x80`) |

## Approach (chosen)

Model the three buttons as new **`G13Key`** variants (`Btn1`, `Btn2`, `Stick`) — the enum
becomes "any G13 button," not only G1–G22. The parser emits ordinary `KeyDown`/`KeyUp(G13Key)`
events for them, so `DeviceState`, the dispatcher (hold-means-hold), and `Config::get_binding`
all work unchanged. (Rejected: a separate `ThumbButton` type + parallel binding/dispatch path —
much more code for no benefit.)

M1/M2/M3 keep their profile-switch role; MR stays reserved (a future increment could make it a
fourth bindable button under this same model).

## Components

### `src/protocol.rs` (TDD)
- Add `G13Key::Btn1`, `Btn2`, `Stick` (derives unchanged — `Debug, Clone, Copy, PartialEq, Eq, Hash`).
- `ReportParser`: decode byte 7 bits 1/2/3 (`Btn1`=bit1, `Btn2`=bit2, `Stick`=bit3) with the same
  edge-detection used for G-keys, emitting `KeyDown`/`KeyUp(G13Key::…)`. Track a `prev_buttons`
  nibble. Byte 7 bit 0 (MR) and bit 7 (heartbeat) are untouched by this decode (disjoint bits;
  the existing M-key decode still owns bit 0).

### `src/config.rs` (TDD)
- `parse_g13_key`: add `"BTN1"→Btn1`, `"BTN2"→Btn2`, `"STICK"→Stick` (case-insensitive).
- `to_toml` already serializes any `G13Key` via its Debug name (`Btn1`/`Btn2`/`Stick`), which
  `parse_g13_key` reads back (uppercased) — round-trips through save/reload. No other change.

### `src/dispatcher.rs`, `src/device_state.rs` — no change
- The buttons are ordinary `KeyDown`/`KeyUp(G13Key)` events: `DeviceState.pressed` tracks them,
  and the dispatcher applies hold-means-hold (or a media tap) exactly as for G-keys.

### `src/monitor/mod.rs` (manual-verify)
- `THUMB: [G13Key; 3] = [G13Key::Btn1, G13Key::Btn2, G13Key::Stick]`.
- **Bindings tab:** render a "Thumb buttons" section (after the G-key grid) with the same
  editable text-field rows (validation, Save/Revert). The reload builds edit buffers from
  `ROWS` flattened **+** `THUMB`; Save collects all of them.
- **Monitor tab:** a compact `BTN1 · BTN2 · STICK` indicator row near the joystick panel that
  highlights on press (from `snapshot.pressed`), mirroring the M-key indicator.

## Edge cases

- Unbound thumb buttons show `(unmapped)`/`—` and inject nothing (same as an unmapped G-key).
- A thumb button bound to a media key taps; bound to anything else holds (hold-means-hold) —
  inherited from the dispatcher, no special-casing.
- The joystick **click** (`Stick`) is independent of joystick **movement** (bytes 1,2) — they
  decode separately and don't interfere.
- `Stick` pressed while moving the stick emits the click event plus `JoystickMove`s — both flow
  through; the click is edge-detected so a held click emits one KeyDown until release.

## Testing

- **Unit (TDD):** `ReportParser` — `Btn1`/`Btn2`/`Stick` press+release from byte-7 patterns;
  centered idle emits none; a thumb button + a G-key in one report emit both; byte-7 MR/heartbeat
  bits don't leak into thumb events. `config.rs` — `parse_g13_key` accepts `BTN1`/`BTN2`/`STICK`
  (case-insensitive); a `to_toml` round-trip preserves a thumb-button binding.
- **Manual-verify:** editor rows + monitor indicators. **Smoke test:** bind `BTN1`/`BTN2`/`STICK`
  to keys, press each → the bound key injects and the button lights in the Monitor; a held thumb
  button holds its key (hold-means-hold); an unbound one does nothing.

## Out of scope (future)
- MR as a fourth bindable button (same model; deferred).
- Physical-accurate placement of the thumb indicators in the Monitor (a simple row is enough now).
