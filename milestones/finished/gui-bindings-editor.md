# GUI: Bindings editor

- **Status:** finished
- **Date:** 2026-07-05
- **Part of:** "Finish the GUI" — sub-project 1 (Bindings editing).

## Outcome
Hardware-verified. The **Bindings** tab edits the active profile's G-key bindings (text field
+ syntax hints + live validation) and saves them back to the profile `.toml` (whole-profile
serialize; comments not preserved). Spec:
`docs/superpowers/specs/2026-07-05-bindings-editor-design.md`; plan:
`docs/superpowers/plans/2026-07-05-bindings-editor.md`.

- `Profile::to_toml` + `ProfileSet::save_active_bindings` (config layer, tested).
- Editable Bindings tab with per-key text fields, Save/Revert; empty = unmapped.
- **Validation** requires the combo to parse AND the key to be a known key (via
  `build_key_map`), so `ctrl+zzz` is rejected in the editor, not just at injection.
- Saving hot-reloads via the existing watcher; editing follows the active profile
  (switch M-keys / Profiles tab to edit a different one).

## Remaining GUI-completion work
- Capture mode (press keys to record a combo).
- Joystick editing (Settings); deadzone slider.
- Profile file management (New/Rename/Delete; reassign files to M1/M2/M3).
- Editing a non-active profile without switching to it (profile-to-edit selector).
- Comment-preserving saves (`toml_edit`); deterministic `[keys]` output order.
- LCD tab — deferred to the v0.4 LCD output protocol.
