# LCD Display (v0.4) — Design

- **Date:** 2026-07-16
- **Milestone:** `milestones/open/v0.4-lcd.md`
- **Status:** approved, ready for implementation plan

## Goal

Drive the G13's 160×43 monochrome LCD as a heads-up display: show the active
profile and live feedback of the last button pressed. Reuses the USB-write
infrastructure and shared-cell + poller pattern built for the RGB backlight.

## Hardware constraints (from community reverse-engineering / g13d)

- The LCD is **160×43 pixels, 1-bit** (monochrome). It is addressable as 160
  columns × 48 rows organized in **6 vertical "pages" of 8 pixels** (SSD1306-style);
  only 43 of the 48 rows are visible.
- A frame is **992 bytes**: a 32-byte header (`buf[0] = 0x03`, rest zero) followed
  by **960 bytes** of page-packed pixel data.
- Pixel `(x, y)` maps to **bit `y % 8` of byte `32 + x + (y / 8) * 160`**.
- The frame is written to **OUT endpoint `0x02`** via an interrupt (possibly bulk)
  transfer. Some drivers issue a one-time init control transfer
  (`bmRequestType=0, bRequest=9, wValue=1, wIndex=0`, no data) before the first frame.
- Like all USB code in this project, the transfer + init get **manual hardware
  verification**, no unit tests. Exact endpoint type and the init transfer are the
  hardware-risk items to confirm in the smoke test.

## Content & layout

Fixed layout (not user-configurable in v0.4). Rendered with an embedded 5×7 ASCII
font (~26 chars per line); the profile name is drawn at 2× scale.

```
 G13 Driver          ■ ACTIVE     y0   title (left) + mode box (right)
 ─────────────────────────────    y9   divider rule
 M2   Media                       y12  active M-slot label + profile name (2× height)
 G12   ctrl+c   Copy              y30  last action: button · combo · label
```

- **Title + mode:** "G13 Driver" left; a small filled box + `ACTIVE` (filled) or
  `DRY-RUN` (hollow) right. In headless mode the mode is always Active.
- **Divider** rule under the title.
- **Profile line:** active M-slot label (`M1`/`M2`/`M3`) + profile name at 2× scale.
  An unassigned slot shows `(empty)`.
- **Last-action line:** the last pressed **bindable discrete button** (G-key or thumb
  button) with its bound combo and label — resolved from the profile active **at press
  time** and persisted until the next press. `(unbound)` when the key has no binding;
  the label is omitted when absent. The **joystick and M-keys are excluded** from this
  line (joystick is continuous; M-keys drive the profile line above).
- Long strings are truncated to fit their region.

## Architecture

```
ProfileSet (slot+name) ┐
mode (Active/Dry-run)  ┼─> lcd::render(model) -> Framebuffer ─┬─(pack)─> Arc<Mutex<frame>> ─> reader ─> EP 0x02
last-action cell ──────┘                                     └─(paint)─> GUI LCD-tab preview
```

A ~150 ms poller rebuilds the model → framebuffer → packed 992-byte frame and
publishes it into a shared cell the reader thread consumes (diff-on-change,
re-send on reconnect). Same shape as the RGB `LedState` path.

## Components

### `src/lcd/mod.rs` + `src/lcd/font.rs` — pure logic (TDD)

- `Framebuffer` — a 160×43 pixel grid with `set_pixel(x, y, on)`, bounds-safe;
  `draw_char`, `draw_text` (optional 2× scale), `draw_hline`, `fill_rect`.
- `font.rs` — a public-domain 5×7 ASCII font (`0x20‥0x7E`) as a `const` byte table.
  No external dependency.
- `LcdModel { title: String, mode: Mode, slot: MKey, profile_name: Option<String>,
  last: Option<LastAction> }`; `LastAction { button: String, combo: Option<String>,
  label: Option<String> }`; `enum Mode { Active, DryRun }`.
- `render(&LcdModel) -> Framebuffer` — lays out the four regions; truncates overflow.
- `pack(&Framebuffer) -> [u8; 992]` — header + page-packed 960-byte body per the
  mapping above. This is the highest-value unit test target.
- `capture(event: &G13Event, profiles: &Arc<RwLock<ProfileSet>>, cell: &Arc<Mutex<Option<LastAction>>>)`
  — on a `KeyDown` for a bindable discrete button, resolve combo + label from the
  active profile and store into `cell`. No-op for joystick/M-keys/KeyUp.
- `spawn_poller(profiles, mode, last_cell, frame_cell)` — build model → render → pack →
  publish, every ~150 ms.

### `src/usb.rs` — write path (manual-verify)

- `UsbReader::run` gains a shared `Arc<Mutex<[u8; 992]>>` (or `Arc<Mutex<Vec<u8>>>`)
  frame cell (alongside the existing `LedState` cell).
- On connect: best-effort LCD init control transfer; warn-and-continue on failure.
- `apply_lcd`: diff the desired frame against a local `last`; on change,
  `write_interrupt(0x02, &frame, timeout)`. Warn-and-continue; re-send on reconnect
  (local `last` resets each fresh `run`). No panics.

### Wiring — `src/runtime.rs`, `src/monitor/mod.rs`

- Both runtimes create the frame cell + last-action cell, spawn `lcd::spawn_poller`,
  pass the frame cell to `reader.run`, and call `lcd::capture` per event in their
  event loop. Headless passes `Mode::Active`; the GUI derives mode from its dry-run flag.

### GUI preview — `src/monitor/mod.rs` `render_lcd`

Build the same `LcdModel` from live state, call `lcd::render`, and paint each pixel as
a scaled rect (~3×) onto the LCD-tab canvas — pixel-exact with the hardware, replacing
the current placeholder. Verifiable without a G13.

## Error handling

- USB init / frame-write failures → `log::warn!` and continue.
- No device → existing supervisor retry; frame writes skipped until a handle exists.
- Rendering never panics; `set_pixel` is bounds-safe; text is truncated, not overflowed.

## Testing

**Pure unit tests (TDD):**
- `pack`: buffer length 992, header `buf[0]=0x03`, a set pixel `(x,y)` lands in
  bit `y%8` of byte `32 + x + (y/8)*160`; corners and page boundaries.
- `Framebuffer`: `set_pixel` bounds safety; `draw_hline`/`fill_rect`.
- Font: `draw_char` produces the expected glyph pixels; `draw_text` advances; 2× scale.
- `render`: representative models — assigned vs empty slot, bound/unbound/labelled
  last action, Active vs Dry-run mode, truncation of long names/combos.
- `capture`: G-key/thumb KeyDown stores resolved action; joystick/M-key/KeyUp are no-ops;
  binding resolved from the active profile.

**Manual hardware smoke test (documented in the milestone):**
- Text renders on the physical LCD (init + endpoint confirmed).
- Profile line updates on M-key/profile switch.
- Last-action line updates on G-key/thumb press and persists.
- Mode reflects Active/Dry-run.
- Survives unplug/replug; no G13 → app runs normally, warnings only.

## Out of scope (YAGNI)

- Clock / CPU / system metrics (milestone lists these as optional; deferred).
- Configurable or per-profile LCD layouts/content.
- Joystick or M-keys in the last-action line.
- Graphics beyond text, the divider rule, and the mode box.
- Multiple font sizes beyond the 1× base and 2× profile-name scale.
