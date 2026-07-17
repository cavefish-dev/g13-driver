# Unify Bindings-tab row style

- **Status:** finished
- **Target:** v0.2
- **Updated:** 2026-07-17

## Goal
One consistent row style for all Bindings-tab mappings (G-keys, thumb, joystick),
matching the preferred joystick layout. Purely visual.

## Tasks
- [x] Extract `render_mapping_row` (joystick style); rewire G-key/thumb + joystick rows to it.

## Acceptance
All Bindings-tab rows look identical (right-aligned name → key → mark → label → repeat);
no behavior/persistence change.

**Smoke PASSED 2026-07-17:** all Bindings rows visually identical (right-aligned name → key → mark → label → repeat); edit+save unchanged.

## Notes
- Design: `docs/superpowers/specs/2026-07-17-unify-binding-rows-design.md`.
- Follow-up from user feedback on the joystick-labels-repeat work.
