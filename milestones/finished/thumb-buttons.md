# Thumb buttons (near-joystick) bindable

- **Status:** finished
- **Date:** 2026-07-07

## Outcome
Hardware-verified. The two buttons next to the joystick and the joystick click are now
first-class bindable buttons and shown in the GUI. Spec:
`docs/superpowers/specs/2026-07-05-thumb-buttons-design.md`; plan:
`docs/superpowers/plans/2026-07-05-thumb-buttons.md`.
- `G13Key` gained `Btn1`, `Btn2`, `Stick`; the parser decodes byte 7 bits 1/2/3 into ordinary
  KeyDown/KeyUp events, so the dispatcher (hold-means-hold), DeviceState, Config, and the editor
  all handle them unchanged.
- Bind names: `BTN1`, `BTN2`, `STICK`. Shown in the Bindings tab (Thumb buttons section) and the
  Monitor `Thumb:` indicator.
- Verified: bind BTN1/BTN2/STICK to keys → they inject, light up in the Monitor, and hold-means-hold.

## Follow-ups
- MR as a fourth bindable button (same model; deferred).
- **Auto-repeat (typematic) for held keys** — a held key currently injects a single key-down
  (held state; correct for games, one char in text) but does NOT auto-repeat like a physical
  keyboard. Adding a repeat timer (initial delay + repeat rate) for held keys would match a real
  keyboard for text fields. Applies to ALL hold-means-hold keys (G-keys + thumb buttons), not
  just this feature. Next up.
