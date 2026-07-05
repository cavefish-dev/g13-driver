# Hold-means-hold G-keys + multimedia keys

- **Status:** finished
- **Date:** 2026-07-05

## Outcome
Hardware-verified. G-key bindings are now hold-means-hold (held while the G-key is held),
enabling chording, held modifiers, and held movement keys. Multimedia keys were added to the
key map and are the tap-only exception. Spec:
`docs/superpowers/specs/2026-07-05-hold-means-hold-and-media-keys-design.md`; plan:
`docs/superpowers/plans/2026-07-05-hold-means-hold-and-media-keys.md`.
- `KeyCombo.key` is `Option<String>` (modifier-only combos like `shift` / `ctrl+shift`).
- Injector gained `combo_down`/`combo_up`; the dispatcher tracks `held_keys` per G-key
  (KeyDown holds or taps a media key, KeyUp releases); `release_held` lifts held G-keys +
  joystick on Dry-run/disconnect/shutdown; profile switch releases only the joystick.
- Media keys: playpause, nexttrack/next, prevtrack/prev, mediastop, volup/volumeup,
  voldown/volumedown, mute.
- Verified on hardware: hold G1=shift chords, hold G2=w held with no stuck key, G3=playpause
  taps once, G4=ctrl+c still copies, and w releases on Active→Dry-run and on disconnect.

## Follow-ups
- Ctrl+C / force-kill graceful release (console control handler) — still open.
- Show the two near-joystick buttons (joystick click + second thumb button) in the GUI —
  needs a quick capture of the second button's bit, then decode + display.
- More system keys (brightness, launch keys) via the same tap-only mechanism.
