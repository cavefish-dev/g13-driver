# "g13" Icon Identity + Help Tab — Design

- **Date:** 2026-07-17
- **Milestone:** `milestones/open/icon-help-tab.md` (new)
- **Status:** approved, ready for implementation plan

Two app-shell additions: a consistent **"g13"** icon (rendered from our embedded
console font) across the tray, window, and exe; and a **Help** tab.

## Feature 1 — "g13" icon identity

### Shared runtime renderer — `src/icon.rs`

`render_g13_rgba(scale: u32, fg: [u8; 4], bg: [u8; 4]) -> (Vec<u8>, u32, u32)`:
- Draws the three glyphs `g`, `1`, `3` (via `crate::lcd::font::glyph`) into an RGBA
  buffer: `fg` where a glyph bit is set, `bg` elsewhere.
- Layout: 3 chars × (5 glyph cols + 1 gap) = 17 cols × 8 rows, each pixel a
  `scale`×`scale` block, plus a small uniform padding. Returns `(rgba, w, h)`.
- Pure and unit-testable (a known glyph's set bits land at the expected RGBA offsets).

### Tray icon — `src/tray.rs`

Replace the flat solid-color square (`icon_rgba`/`make_icon`) with "g13" drawn **in
the current status color** on a dark background — so the icon is *both* identity and
status. `IconState` → foreground color (Problem = red, Active = green, Dry-run = grey),
dark background (e.g. near-black). Keeps the existing per-state icon swap (`update`
re-renders on state change).

### Window / title-bar icon — `src/monitor/mod.rs`

Set `ViewportBuilder::with_icon(egui::IconData { rgba, width, height })` from a fixed
"g13" render (a neutral fg, e.g. the Active green or white) at a window-appropriate size.

### Exe / taskbar icon — `build.rs` + `winres`

Embed a `.ico` so Explorer and the taskbar show "g13":
- `build.rs` (Windows only) renders "g13" from the same three glyphs into RGBA at a few
  sizes (16/32/48/256), encodes an ICO with the **`ico`** crate to `${OUT_DIR}/g13.ico`,
  and embeds it via the **`winres`** crate (`WindowsResource::new().set_icon(...).compile()`).
- To avoid duplicating glyph data, the three `g`/`1`/`3` glyph constants live in a tiny
  file (`src/g13_glyphs.rs`) `include!`'d by both `build.rs` and `src/icon.rs`; each has
  its own ~10-line blit (build.rs cannot call crate code).
- **Deps:** `winres` + `ico` as `[build-dependencies]` (both pure-Rust).

**⚠️ Toolchain risk:** `winres` compiles the resource with **`windres`** (MinGW) on the
GNU target. Strawberry's MinGW usually provides `windres`; if it is missing or fails, the
exe-icon build step fails. Mitigation: the exe-icon work is isolated to `build.rs`; verify
`windres` availability first, and if it can't be made to work, fall back to the
`embed-resource` crate or **defer just the exe icon** — the tray + window "g13" icons
still ship (they need no build resource). Do not let the exe icon block the rest.

## Feature 2 — Help tab

New `Tab::Help` (added to the `Tab` enum + the tab list) with `render_help(ui)` — a
`ScrollArea` of headings + short paragraphs covering:
- **What it is:** open-source G13 driver; Active vs Dry-run; press **MR** to toggle
  (mode box + tray reflect it; MR LED lights in dry-run).
- **First-time setup:** install the WinUSB driver via Zadig (link to `docs/zadig-setup.md`).
- **Tabs walkthrough:** Monitor (live keypad view), Profiles (M1/M2/M3 slots + folder),
  Bindings (combos, per-key labels + repeat, joystick directions with labels + repeat),
  Catalog (download/revert community profiles), LCD (per-line content config + preview),
  Settings (backlight color/brightness/M-key indicator, joystick deadzone, launch-at-login,
  updates).
- **Tray + autostart:** closing/minimizing hides to the tray; quit from the tray.
- **Config file:** the app loads `config.toml` next to the exe (or CWD); GUI edits persist there.

Static content — no config, no new state.

## Testing

- **Unit (TDD):** `icon::render_g13_rgba` — buffer size = `(w*h*4)`; a chosen glyph's set
  bit produces `fg` at the right pixel and `bg` elsewhere; `scale` doubles blocks.
- **Manual smoke:** tray shows "g13" in the status color and updates on Active↔Dry-run↔
  disconnect; the window title-bar/alt-tab icon shows "g13"; the built exe shows "g13" in
  Explorer/taskbar; the Help tab renders and scrolls.

## Out of scope

- Animated / multi-color icons; localization of help text; in-app interactive tutorials.
- Any change to existing tab behavior.
