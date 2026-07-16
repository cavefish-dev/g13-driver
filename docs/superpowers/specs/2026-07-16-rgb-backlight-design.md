# RGB Backlight (v0.5) — Design

- **Date:** 2026-07-16
- **Milestone:** `milestones/open/v0.5-rgb.md`
- **Status:** approved, ready for implementation plan

## Goal

Control the G13's keypad backlight color and the M-key mode indicators, driven by
the active profile. First of two features (RGB now, LCD as a follow-up) that both
need a new USB **write** path — RGB builds that path cheaply before the heavier LCD
work.

## Hardware constraints (from community reverse-engineering)

- The keypad backlight is a **single lighting zone**: one RGB color for the whole
  pad (it also tints the LCD backlight). There is **no per-key control** — every
  G-key lights together with the same color and brightness. "Off" = color `(0,0,0)`.
- **No separate brightness register**: brightness is the magnitude of the RGB values.
  We honor a global brightness as a software scalar applied before sending.
- **M1/M2/M3/MR LEDs** are separate **monochrome on/off indicators**, set via a
  bitmask (1=M1, 2=M2, 4=M3, 8=MR). Fixed color — cannot be recolored.
- USB commands are `SET_REPORT` control transfers (`bmRequestType=0x21`,
  `bRequest=9`): color via `wValue=0x0307`, M-LEDs via `wValue=0x0305`.

These are confirmed against g13d / golgote-G13 community notes and the ArchWiki G13
page, but — like all USB code in this project — get **manual hardware verification**,
no unit tests on the transfer itself.

## Behavior model

| Thing | Hardware capability | Configured as |
|---|---|---|
| Keypad backlight color | RGB, whole pad | **per M-slot** (m1/m2/m3), stored in global `config.toml` |
| Brightness | RGB magnitude (software scalar) | **global** |
| M1/M2/M3 LED | on/off bitmask | **global toggle**: light the active profile's M-key |

- A slot with no color override uses the **global default color** (starts white).
- Color is driven identically in **dry-run and active** mode — it's the keypad's own
  light, not injection into other apps.
- On quit: leave the last color as-is. No G13 connected: silent no-op.

## Config schema

New `[backlight]` section in the global `config.toml`. Profile files are untouched.

```toml
[backlight]
default_color  = "#FFFFFF"   # fallback for any slot without an override; starts white
brightness     = 1.0         # global scalar 0.0-1.0 (0 = off)
mkey_indicator = true        # light the active profile's M-key LED
m1_color       = "#FF0000"   # optional per-slot override; absent = default_color
m2_color       = "#3030FF"
# m3_color unset -> inherits default_color
```

- Colors are `#RRGGBB` hex.
- A missing `[backlight]` section → all defaults: `default_color` white,
  `brightness` 1.0, `mkey_indicator` true, no slot overrides.

## Components

### `src/led/mod.rs` — pure logic (TDD)

- `Color(u8, u8, u8)` with hex parse/format (round-trip tested).
- `LedState { rgb: (u8, u8, u8), mkeys: u8 }` — the resolved hardware state.
- `resolve(active: MKey, cfg: &BacklightConfig) -> LedState`:
  - color = `slot_colors[slot_index(active)]` or `default_color`,
    each channel multiplied by `brightness` and rounded.
  - `mkeys` = the bit for the active slot (1/2/4) when `mkey_indicator` is on,
    else 0. `MKey::MR` → no bit (matches the existing `slot_index` returning `None`).
- Packet builders (pure, tested):
  - color → `[0x05, r, g, b, 0x00]`
  - M-LEDs → `[0x05, mask, 0x00, 0x00, 0x00]`

### `src/config.rs` — `BacklightConfig`

- `BacklightConfig { default_color: Color, brightness: f32, mkey_indicator: bool, slot_colors: [Option<Color>; 3] }`,
  parsed from the `[backlight]` section of `config.toml` and exposed off `ProfileSet`.
- Setters mirror the existing joystick-deadzone pattern:
  - `set_backlight_*` — update in-memory (so the poller reconciles immediately).
  - `persist_backlight()` — write back to `config.toml`.
- Round-trip and defaults are unit-tested.

### `src/usb.rs` — write path (manual-verify)

- The reader thread takes a shared `Arc<Mutex<LedState>>` "desired state" cell.
- Each loop tick (the read loop already wakes every 100 ms on timeout), compare
  desired vs a locally-remembered `last_applied`; on diff, issue the two control
  transfers (color `0x0307`, M-LEDs `0x0305`).
- On each (re)connect, reset `last_applied` so the current desired state is
  re-applied automatically after replug.
- Transfer failures **log a warning and continue** (matches the injection error
  policy — a missed LED update is recoverable; a crashed driver is not). No panics.

### Trigger — diff poller

- A small thread reads `Arc<RwLock<ProfileSet>>` every ~150 ms, computes
  `resolve(active, backlight_cfg)`, and writes it into the desired-state cell.
- Every change source — device M-key press, GUI profile switch, GUI color/brightness
  edit, config hot-reload — just updates config/state; the poller reconciles them all
  uniformly, so no per-source wiring is needed.
- Both runtimes (`runtime::run_headless` and the GUI `monitor::run`) spawn the reader
  supervisor + this poller and share the one desired-state cell. The reader supervisor
  is factored so both pass the cell in.

### GUI (`src/monitor/mod.rs`)

- **Profiles tab:** each M-slot row (M1/M2/M3) gains a color swatch + "Use default"
  checkbox. Custom color writes `[backlight] mN_color`; "Use default" removes the key.
  Persists immediately + updates in-memory (mirrors the deadzone slider).
- **Settings tab:** a "Backlight" block — default color picker (white), brightness
  slider (0–100%), "Light active profile's M-key" checkbox. Same persist pattern.
- The existing `Tab::Lcd` scaffold is left for the follow-up.

## Data flow

```
config.toml [backlight] ─┐
active M-slot ───────────┼─> led::resolve() ──(poller, ~150ms)──> Arc<Mutex<LedState>>
GUI edits / hot-reload ──┘                                              │
                                                                        v
                            reader thread (owns USB handle) ── control transfers ──> G13 LEDs
                            (diffs vs last_applied each tick; re-applies on reconnect)
```

## Error handling

- USB control-transfer failure → `log::warn!` and continue.
- No device / device open failure → the existing supervisor retry loop; LED writes
  are simply skipped until a handle exists.
- Malformed hex / out-of-range brightness in config → fall back to the default for
  that field and warn; never panic.

## Testing

**Pure unit tests (TDD):**
- `resolve`: default fallback, per-slot override, brightness scaling (incl. 0 = off),
  M-key bitmask per slot, `MR` → no indicator.
- `Color` hex parse/format round-trip; malformed hex handling.
- Packet byte layout for color and M-LED commands.
- `config.rs`: `[backlight]` parse, persist round-trip, missing-section defaults.

**Manual hardware smoke test (documented in the milestone):**
- Color changes on profile switch (device M-key and GUI).
- Brightness slider dims the pad; 0 turns it off.
- M-key LED tracks the active slot.
- Survives unplug/replug (state re-applied).
- No-ops cleanly with no G13 connected.

## Out of scope (YAGNI)

- Lighting effects / animation (breathing, cycling) — static color only.
- Per-key colors — impossible on this hardware.
- LCD — the follow-up feature (v0.4), separate spec.
- MR slot color/profile — MR is not a profile slot.
- Turning the backlight off on quit — leave the last color.
