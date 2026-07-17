# MR Mode Toggle + LCD Filename Stem — Design

- **Date:** 2026-07-17
- **Milestone:** `milestones/open/pre-release-polish.md` (new)
- **Status:** approved, ready for implementation plan

Two small pre-release enhancements.

## Feature 1 — MR toggles dry-run/active (GUI + headless)

Repurpose the currently-inert MR mode key to toggle injection on/off, in **both**
runtimes, so a future headless Linux service is fully controllable from the device.

**Background:**
- The dispatcher already no-ops MR (`handle_mkey`: `if m == MKey::MR { return; }`), so
  it fires no keystroke — free to repurpose at the event-loop level.
- Dry-run/active is an `AtomicBool` (`dry_run`). The GUI tray "Active" toggle flips it
  with `store(!load)` + repaint; a watcher persists `[app] start_active`, and
  `consumer_loop` releases held keys on an active→dry transition.
- **Headless has no dry-run concept today** — `run_headless` always injects.

**GUI:** in `consumer_loop`, on `G13Event::MKeyDown(MKey::MR)`, flip `dry_run`
(`store(!load)`) and `ctx.request_repaint()`. Existing persistence + held-key-release
logic handle the rest.

**Headless (new capability):** `run_headless` gains a `dry_run: Arc<AtomicBool>`
starting Active (`false`). The dispatch loop mirrors the GUI's:
- compute `active = !dry_run.load(Relaxed)`;
- release held keys on an active→dry transition (track `was_active`);
- only call `dispatcher.handle(event)` when `active`;
- `MKeyDown(MR)` flips the flag.
This same flag feeds the LCD poller as the mode source (replacing the always-false
placeholder introduced with the LCD feature), so the headless mode box reflects it.

**Unchanged / out of scope:**
- M1/M2/M3 profile switching already works in both runtimes.
- `capture` ignores M-keys, so MR never appears in the LCD last-action line.
- Headless boots Active every launch (MR toggles live; not persisted). Persisting
  headless mode across restarts is out of scope; `[app] start_active` stays
  GUI-managed.

## Feature 2 — LCD shows profile filename without extension

The LCD currently shows `ProfileSet::active_name()`, which is the raw manifest
filename *with* extension (e.g. `basic.toml`). Show the stem (`basic`) instead.

- Add `ProfileSet::active_name_stem(&self) -> Option<&str>` =
  `self.active_name().map(|n| n.trim_end_matches(".toml"))`.
- Use it instead of `active_name()` when building the `LcdModel.profile_name` in both
  `lcd::spawn_poller` and the GUI `render_lcd` preview.
- Only the LCD changes; the Profiles tab keeps showing `[meta]` display names.

## Testing

- **Unit:** `active_name_stem` strips a trailing `.toml`; a name already without an
  extension is returned unchanged; `None` stays `None`.
- **Event-loop wiring** (MR toggle, headless dry-run gating): no new unit test — it
  reuses the established GUI dry-run pattern; verified by the hardware smoke test.
- **Hardware smoke:** press MR (device) in the GUI → injection stops/starts and the
  mode box flips ACTIVE↔DRY-RUN; run `--headless` and confirm MR toggles injection
  there too; confirm the LCD shows the filename without `.toml`.

## Out of scope

- Persisting headless mode across restarts.
- MR doing anything inside the dispatcher.
- Any change to M1/M2/M3 behavior or to the Profiles/Settings tabs.
