# GUI monitor (dry-run test tool)

- **Status:** finished
- **Target:** (pulled forward from v1.0 GUI at user request)
- **Updated:** 2026-07-04

## Goal
Default-launch egui/eframe window: live G13 monitor (G-keys + joystick) + mapping
preview + Dry-run/Active toggle, so the driver can be tested without injecting into
other Windows apps. Tray + full configurator remain future work.

## Outcome
Shipped and **hardware-verified on the real G13**. Built via subagent-driven development
on branch `feat/gui-monitor` (6 tasks, each reviewed; final whole-branch review).
Spec: `docs/superpowers/specs/2026-07-01-gui-monitor-dry-run-design.md`;
plan: `docs/superpowers/plans/2026-07-03-gui-monitor-dry-run.md`.

- `DeviceState` reducer (`src/device_state.rs`, unit-tested) reconstructs live input from
  the `G13Event` stream.
- `runtime` module (`src/runtime.rs`) shares startup wiring; `--headless` preserves the
  console driver, default launch opens the GUI.
- Consumer thread updates the monitor always and injects via the `Dispatcher` only when
  **Active**; **Active→Dry-run** and **disconnect** call `release_held()` (no stuck keys).
- `src/monitor/` (eframe): physical-layout key grid (rows 7/7/5/3, short rows centered),
  joystick box (deadzone circle + live position dot + WASD highlight), status footer,
  and a **Retry connection** button shown only while disconnected.
- First launch is **Dry-run** (safe): moving the stick / pressing keys updates the monitor
  but injects nothing until you switch to Active.
- Verified on hardware: keys highlight with correct bindings, joystick dot + WASD track,
  Dry-run injects nothing, Active injects, toggle-back releases held keys, unplug→Retry
  reconnects. eframe 0.31.1 builds cleanly on the GNU toolchain.

## Follow-ups
- System tray + minimize-to-tray + start-in-tray + remember-last-state (the original
  "minimize to tray" vision — next GUI sub-project).
- Automatic reconnect polling (beyond the manual Retry button).
- M-keys / joystick-click in the monitor (pairs with the M-key decode sub-project; bytes
  6/7 layout already captured — see `milestones/finished/02-hardware-bringup.md`).
- Ctrl+C stuck-key handler (tracked from the joystick sub-project; the GUI's Dry-run
  default mitigates it during testing).
