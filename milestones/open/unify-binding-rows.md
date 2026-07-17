# Unify Bindings-tab row style

- **Status:** open
- **Target:** v0.2
- **Updated:** 2026-07-17

## Goal
One consistent row style for all Bindings-tab mappings (G-keys, thumb, joystick),
matching the preferred joystick layout. Purely visual.

## Tasks
- [ ] Extract `render_mapping_row` (joystick style); rewire G-key/thumb + joystick rows to it.

## Acceptance
All Bindings-tab rows look identical (right-aligned name → key → mark → label → repeat);
no behavior/persistence change.

## Notes
- Design: `docs/superpowers/specs/2026-07-17-unify-binding-rows-design.md`.
- Follow-up from user feedback on the joystick-labels-repeat work.
