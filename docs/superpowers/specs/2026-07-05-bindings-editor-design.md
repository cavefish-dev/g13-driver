# Bindings editor — design

- **Status:** approved (design)
- **Date:** 2026-07-05
- **Part of:** "Finish the GUI" — sub-project 1 of the GUI-completion work (Bindings editing
  first; profile management and Settings/tray follow; the LCD tab stays a labeled
  placeholder until the v0.4 LCD output protocol exists).
- **Scope:** Make the **Bindings** tab edit the **active profile's** G-key bindings and save
  them back to that profile's `.toml` file (which then hot-reloads). Text-field input with
  syntax hints and live validation. Joystick editing is out of scope (Settings/future).

## Goal

Edit key bindings in the GUI instead of hand-editing TOML: click into a G-key's field, type
a combo (`ctrl+c`), see it validated live, and Save it to the active profile file. Empty =
unmapped. Editing follows the active profile (switch profiles via M-keys / the Profiles tab
to edit a different one).

## Persistence approach (chosen)

**Serialize the whole profile** (Approach 1): on Save, rebuild the profile's TOML from its
in-memory keys + joystick and write it. No new dependency. **Trade:** hand-written comments
and formatting are lost when the GUI first rewrites a profile file (the file becomes
GUI-managed). `toml_edit`-based comment-preserving edits are a possible later upgrade.

## Components

### `src/config.rs` — mutate + serialize + persist (TDD)
- Add `#[derive(Serialize)]` to `RawConfig` and `RawJoystick` (alongside `Deserialize`).
- `Profile`:
  - `bindings(&self) -> &HashMap<G13Key, String>` (read for the editor).
  - `set_bindings(&mut self, bindings: HashMap<G13Key, String>)` — replace the `[keys]` set
    (joystick untouched).
  - `to_toml(&self) -> Result<String>` — build a `RawConfig` from the profile (G-keys →
    `"G1"` strings via the inverse of `parse_g13_key`; joystick → `RawJoystick`) and
    `toml::to_string`.
- `ProfileSet`:
  - `active_path(&self) -> PathBuf` — `profiles_dir` + active filename (legacy: the config
    file itself).
  - `save_active_bindings(&mut self, bindings: HashMap<G13Key, String>) -> Result<()>` —
    replace the active profile's `[keys]` in memory, then write `to_toml()` to `active_path()`.

### `src/monitor/mod.rs` — the editor (manual-verify)
- `MonitorApp` gains edit state: `edits: HashMap<G13Key, String>` and `edits_for: Option<String>`
  (the profile the buffers belong to) plus a transient `save_status: Option<String>`.
- `render_bindings` becomes an editor:
  - **Header:** `Editing profile: <active name>`.
  - **Hints line:** combo syntax + available keys + examples + "Empty = unmapped".
  - **Rows (scrollable, G1–G22):** `G1` + a text field bound to `edits[key]` + a validity mark
    (green if `KeyCombo::parse` succeeds, red if not, dim if empty).
  - **Reload rule:** when the tab renders and `edits_for != active_name`, reload `edits` from
    the active profile's bindings and set `edits_for` (so opening the tab or switching profiles
    resets the buffers to that profile's current bindings; unsaved edits are discarded on
    switch).
  - **Save:** enabled only when every non-empty field parses; collects non-empty buffers into a
    `HashMap<G13Key, String>` and calls `save_active_bindings`, setting `save_status` to "saved"
    or the error.
  - **Revert:** reload buffers from the active profile (`edits_for = None` to force reload).
- Validation reuses `crate::injector::KeyCombo::parse`, so "green here" == "injects at runtime".

## Error handling & edge cases

- **Invalid combo:** red mark; **Save disabled** until all non-empty fields parse — an
  unparseable binding can never be written.
- **Empty field:** the key is unmapped (omitted from the saved `[keys]`).
- **Write failure** (locked/permission): `log::warn` + an error line in the tab; no crash; the
  buffers keep the edits for retry.
- **Switch profile/M-key with unsaved edits:** buffers reload to the new active profile,
  discarding unsaved edits (simple; an "unsaved changes" guard is a later nicety).
- **Manifest vs legacy:** manifest mode writes the active profile file (`profiles/…toml`), the
  manifest `config.toml` untouched; legacy mode rewrites `config.toml` as a valid bare-`[keys]`
  file. Joystick section carried through unchanged in both.
- **Self-triggered reload:** Save writes the file → the existing watcher reloads identical
  content (harmless; keeps memory and disk identical).

## Testing

- **Unit (TDD):** `Profile::to_toml` round-trip (set bindings → serialize → reload → bindings +
  joystick preserved); `ProfileSet::save_active_bindings` (writes the active file so a fresh
  load shows the new bindings; joystick preserved; manifest + other profile files untouched —
  temp-dir file test). Combo validation already covered by `KeyCombo::parse` tests.
- **Manual-verify:** the editor rendering, Save/Revert, live validation. **Smoke test:** edit a
  binding → Save → file changes + hot-reload + the new binding injects; empty a field to unmap;
  an invalid combo blocks Save; Revert discards; edit a non-active profile by switching to it.

## Out of scope (future increments)
- **Capture mode** (press keys to record a combo) — a follow-up now that the edit/save plumbing
  exists.
- **Joystick editing** (deadzone/keys) — belongs to Settings.
- **Editing a non-active profile without switching to it** — a "pick a profile to edit"
  selector.
- **Comment-preserving saves** (`toml_edit`).
- Profile file management (New/Rename/Delete, reassigning files to M-slots) — the next
  GUI-completion sub-project.
