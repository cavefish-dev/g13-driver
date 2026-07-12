# Background app: tray, auto-start, single-instance

- **Status:** finished
- **Date:** 2026-07-12

## Outcome
Hardware-verified. The driver runs as a background app. Spec:
`docs/superpowers/specs/2026-07-10-background-app-design.md`; plan:
`docs/superpowers/plans/2026-07-10-background-app.md`. This is MVP sub-project #1 of 3.

- **Window decoupled from process:** Close (X) / Minimize hide to the tray (driver keeps
  injecting); **Quit** from the tray is the only exit. Verified: hide/show cycle, Quit while shown
  and while hidden.
- **3-state tray status icon** (red problem > green Active > grey Dry-run), generated in code.
- **Auto-start** at login: opt-in via the HKCU Run key with `--minimized` (`"<exe>" --minimized`);
  toggle mirrored in the tray + Settings, reads/writes the registry live. Verified the entry
  appears/disappears.
- **Single instance:** named mutex; a second launch signals the first (named event) to show its
  window and exits. Verified (second launch does not start a second process).
- **Persisted mode:** last Active/Dry-run saved to `config.toml` `[app] start_active` (format-
  preserving via toml_edit); resumed on next launch. Verified.
- **No console flash** (`windows_subsystem = "windows"`); `--headless` reattaches the parent
  console for logs.
- New deps (Windows-only): `tray-icon`, `winreg`, `toml_edit`.

## Smoke-test fixes (the hard part — eframe + tray integration)
The tray/eframe integration needed several rounds of hardware debugging:
- **eframe `update()` does not run while the window is hidden**, so polling tray events in
  `update()` never fired. Fixed by handling tray + activation via `tray-icon`'s global event
  handlers (which run on the message-loop thread even while hidden).
- **`ViewportCommand::Visible(true)` does not un-hide a window eframe has hidden** (its
  redraw/command processing is paused while hidden). Fixed by showing/hiding the OS window
  **directly via Win32** (`FindWindowW("G13 Monitor")` + `ShowWindow` + `SetForegroundWindow`),
  bypassing eframe. Hide also goes through Win32 so show/hide stay in sync.
- **Quit while hidden** couldn't process the close (update() paused) — fixed by showing the window
  first, then posting `WM_CLOSE` (the quit flag lets the close through).
- **Config not found when launched from another directory** (broke auto-start) — fixed by
  resolving `config.toml` **next to the exe first, then the CWD**.
- **No reconnect on replug** — added an automatic USB **reconnect supervisor** (retries open every
  2s, holds the channel open across reconnects); the manual "Retry connection" button was removed.

## Follow-ups
- **`%APPDATA%\g13-driver\` config location** for real distribution (created on first run;
  profiles could be downloaded from the public GitHub repo). Deferred to the distribution work —
  today config ships next to the exe.
- Tray icon does not visually refresh while the window is hidden (functional state is correct; it
  catches up when shown) — because eframe `update()` is paused while hidden.
- Release held keys on USB disconnect (a G-key held at unplug could stick until next interaction).
- Toast/balloon notifications; profile switching from the tray.
- Next MVP sub-projects: (#2) GitHub Actions CI driven by a `version.txt` semver file that triggers
  releases; (#3) auto-update pulling builds from GitHub Releases.
