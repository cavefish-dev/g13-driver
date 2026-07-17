# "g13" icon identity + Help tab

- **Status:** open
- **Target:** v0.2
- **Updated:** 2026-07-17

## Goal
A consistent "g13" icon (rendered from the embedded console font) across the tray,
window, and exe; and an in-app Help tab.

## Tasks
- [ ] `src/icon.rs` — `render_g13_rgba` (font-based RGBA renderer).
- [ ] Tray icon shows "g13" in the status color.
- [ ] Window/title-bar icon = "g13".
- [ ] Exe/taskbar icon via `build.rs` + `winres` + generated `.ico` (⚠ needs `windres`).
- [ ] Help tab with a usage guide.

## Acceptance
Tray/window/exe all show "g13" (tray in the status color); Help tab renders a usage guide.

## Notes
- Design: `docs/superpowers/specs/2026-07-17-g13-icon-help-tab-design.md`.
- Risk: `winres`/`windres` on the GNU toolchain — verify early; exe icon is deferrable
  without blocking the tray/window icons.
