# "g13" icon identity + Help tab

- **Status:** ongoing
- **Target:** v0.2
- **Updated:** 2026-07-17

## Goal
A consistent "g13" icon (rendered from the embedded console font) across the tray,
window, and exe; and an in-app Help tab.

## Tasks
- [x] `src/icon.rs` — `render_g13_rgba` (font-based RGBA renderer).
- [x] Tray icon shows "g13" in the status color.
- [x] Window/title-bar icon = "g13".
- [x] Exe/taskbar icon via `build.rs` + `winres` + generated `.ico` (⚠ needs `windres`).
- [x] Help tab with a usage guide.

## Acceptance
Tray/window/exe all show "g13" (tray in the status color); Help tab renders a usage guide.

## Notes
- Design: `docs/superpowers/specs/2026-07-17-g13-icon-help-tab-design.md`.
- Risk: `winres`/`windres` on the GNU toolchain — verify early; exe icon is deferrable
  without blocking the tray/window icons.

## Hardware smoke test (manual)
- [ ] Tray icon shows "g13" and its color tracks Active (green) / Dry-run (grey) / disconnected (red).
- [ ] Window title-bar / alt-tab shows the "g13" icon.
- [ ] The built exe shows "g13" in Explorer and the taskbar.
- [ ] The Help tab renders and scrolls with accurate content.
