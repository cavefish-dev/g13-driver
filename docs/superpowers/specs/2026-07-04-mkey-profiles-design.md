# M-key profiles — design

- **Status:** approved (design)
- **Date:** 2026-07-04
- **Part of:** v0.2 (`milestones/open/v0.2-joystick-mkeys-service.md`), sub-project 2 of 3.
- **Scope:** Decode the G13's M-keys and use them to switch between profiles — each
  profile a full binding set (`[keys]` + `[joystick]`) stored in its own file. M1/M2/M3
  select the profile bound to that key; the GUI Profiles tab displays and switches them.

## Model (confirmed)

- Three profiles, bound to **M1/M2/M3**. Each profile is a complete binding set (a `[keys]`
  + `[joystick]` file, identical to today's `config.toml` format).
- A `profiles/` folder holds the profile files (and may hold extra files for a future
  "browse and reassign" feature). `config.toml` becomes a **manifest** naming the folder and
  the file bound to each M-key.
- Startup active profile = **M1**. **MR** is decoded but reserved (no-op) for a future feature.

## Report layout (hardware-verified)

M-keys live in bytes 6–7 (see `milestones/finished/02-hardware-bringup.md`):
M1 = byte 6 bit 5 (`0x20`), M2 = byte 6 bit 6 (`0x40`), M3 = byte 6 bit 7 (`0x80`),
MR = byte 7 bit 0 (`0x01`). Byte 7 bit 7 (heartbeat) and bit 3 (joystick click, out of
scope here) are ignored.

## File layout

```
config.toml            # manifest
profiles/
  default.toml         # M1  ([keys] + [joystick])
  game.toml            # M2
  media.toml           # M3
  …                    # extra files (future browse)
```

Manifest (`config.toml`):
```toml
profiles_dir = "profiles"
m1 = "default.toml"
m2 = "game.toml"
m3 = "media.toml"
```

`profiles_dir` (and thus each profile file) resolves **relative to `config.toml`'s directory**
(the current working directory, where `config.toml` is already loaded from). Each `m1/m2/m3`
value is a filename inside `profiles_dir`.

## Components

### `src/protocol.rs` — M-key decode (TDD, additive)
- `pub enum MKey { M1, M2, M3, MR }`.
- `G13Event` gains `MKeyDown(MKey)` and `MKeyUp(MKey)`.
- `ReportParser` tracks the M-key bits (byte 6 bits 5–7, byte 7 bit 0) with the same
  edge-detection used for G-keys, emitting `MKeyDown`/`MKeyUp`. Byte 7 bits 7 and 3 are
  masked/ignored. G-key and joystick decoding are unchanged.

### `src/config.rs` — Profile / Manifest / ProfileSet (TDD)
- Rename today's `Config` (keys + joystick) to **`Profile`** — unchanged API (`get_binding`,
  `joystick`). `Profile::load(path)` reads one profile file.
- **`Manifest`** — parsed from `config.toml`: `profiles_dir: PathBuf`, `m1: String`,
  `m2: Option<String>`, `m3: Option<String>`. Detection: a top-level `m1` key ⇒ manifest
  mode; a bare `[keys]` with no `m1` ⇒ **legacy mode** (the file is itself the single M1
  profile).
- **`ProfileSet`** — `{ profiles_dir: PathBuf, m1: Profile, m2: Option<Profile>,
  m3: Option<Profile>, active: MKey }` with:
  - `active_profile(&self) -> &Profile` (M1 always present; falls back to M1 if `active`'s
    slot is somehow empty)
  - `set_active(&mut self, MKey) -> bool` (returns false / no-op if that slot is empty or MR)
  - `name(&self, MKey) -> Option<&str>` (bound filename)
  - `available(&self) -> Vec<String>` (`.toml` files in `profiles_dir`)
  - `load(config_path) -> Result<ProfileSet>` (manifest or legacy); active = M1.

### `src/dispatcher.rs` — dispatch from the active profile + switching
- `Dispatcher` holds `Arc<RwLock<ProfileSet>>` (was `Arc<RwLock<Config>>`).
- `handle_key` → `active_profile().get_binding(key)`; `handle_joystick` →
  `active_profile().joystick()`. Otherwise unchanged.
- `MKeyDown(M1/M2/M3)` → `release_held()` (a new profile may rebind the joystick, so lift any
  held movement key first), then `set_active(mkey)`; log `profile -> <name>` or
  `no profile bound to <mkey>`. `MKeyDown(MR)` and all `MKeyUp` ignored.

### `src/runtime.rs` — wiring
- `load_config_and_watch` builds the `ProfileSet` and returns `Arc<RwLock<ProfileSet>>`. The
  watcher observes `config.toml` **and** `profiles_dir` (recursive). `run_headless` and the
  GUI consumer are otherwise unchanged (new `MKeyDown` events flow through `dispatcher.handle`).

### `src/device_state.rs` — M-key display state (TDD)
- Add `mkeys: HashSet<MKey>`; `apply` handles `MKeyDown`/`MKeyUp`.

### `src/monitor/mod.rs` — GUI
- **Header:** `Profile: <active name>` next to the title.
- **Monitor tab:** a small M-key indicator row (M1 M2 M3 MR) highlighting pressed keys.
- **Profiles tab** (replaces placeholder): the three slots M1/M2/M3, each showing its bound
  filename (or `(unassigned)`), active slot highlighted; **clicking a slot calls
  `set_active`** (same as pressing the M-key). Below, a read-only **"Available in
  profiles/"** list. New/Rename/Delete and reassigning files to slots stay inert
  placeholders (future increment).

## Hot-reload & error handling

- Watch `config.toml` + `profiles_dir` (recursive). On change, rebuild `ProfileSet`,
  **preserving the active slot** if it still exists (else fall back to M1).
- **M1 required:** missing/invalid at startup ⇒ hard error (driver can't run without a
  default). **M2/M3** missing/invalid ⇒ empty slot (warn); switching to it is a no-op.
- **Reload failures are non-destructive:** an invalid edited file keeps the last-good
  `ProfileSet` and logs `profile reload failed: …` (same policy as today's config reload).
- **Legacy `config.toml`** (bare `[keys]`, no `m1`) loads as a single M1 profile — no folder,
  no switching.

## Testing

- **Unit (test-first):** `ReportParser` M-keys (each press/release; idle emits none; M-key +
  G-key together); `Profile::load` + `Manifest` parse (manifest, legacy, missing fields);
  `ProfileSet` (`set_active`, no-op on empty slot / MR, `active_profile`, `name`,
  `available`, active-preserving reload); `DeviceState` M-key apply; dispatcher `MKeyDown`
  switches the active profile, resolves bindings from the **new** profile, and calls
  `release_held` on switch (via `MockInjector` + a two-profile `ProfileSet`).
- **Manual-verify:** GUI rendering; hardware smoke test — press M1/M2/M3 → header profile
  name changes and G-keys inject the **new** profile's bindings; unassigned slot is a no-op;
  hot-reload a profile file live; legacy config still runs.

## Out of scope (future increments)
- Editing bindings in the GUI; creating/renaming/deleting profile files; reassigning which
  file is bound to a slot (browse list is read-only this round).
- M-key **LEDs** (needs the output/LED protocol).
- **MR** behavior (decoded but reserved).
- Per-profile mouse mode is inherited from the existing joystick config (still not
  implemented; a `mode = "mouse"` profile stays inert as today).

## Migration / shipping
- Ship an updated `config.toml` manifest + a `profiles/` folder: `default.toml` (the current
  bindings), plus example `game.toml` / `media.toml`. Existing users with a legacy
  `config.toml` keep working unchanged (single M1 profile).
