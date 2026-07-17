# Unify Bindings-tab Row Style — Design

- **Date:** 2026-07-17
- **Milestone:** `milestones/open/unify-binding-rows.md` (new)
- **Status:** approved, ready for implementation plan

Small GUI-only refactor: make every Bindings-tab mapping row (G-keys, thumb
buttons, joystick directions) use one consistent row style — the joystick style,
which the user prefers.

## Background

Two divergent row layouts exist in `src/monitor/mod.rs`:
- **G-key / thumb** (`render_binding_row`): left-aligned `{key:?}` name → key field →
  validity mark → **repeat checkbox** → **label field** ("label (optional)").
- **Joystick** (inline loop): right-aligned `{name:>5}` name → key field → validity
  mark → **label field** ("label") → **repeat checkbox**.

## Change

Extract a single shared row renderer and use it everywhere:

```rust
fn render_mapping_row(
    ui: &mut egui::Ui,
    name: &str,                 // right-aligned to width 5
    key_buf: &mut String,       // combo/key text
    label_buf: &mut String,     // label text (hint "label (optional)")
    repeat_buf: &mut bool,      // repeat checkbox
    valid_keys: &HashSet<String>,
)
```

Unified layout (joystick style): **name (right-aligned, width 5) → key field →
validity mark (—/ok/bad) → label field ("label (optional)") → repeat checkbox.**

Wiring:
- `render_binding_row` (G-keys + thumb) becomes a thin wrapper: it looks up the
  `edits`/`label_edits`/`repeat_edits` HashMap entries for the key and the
  `{key:?}` name, then calls `render_mapping_row`.
- The joystick loop calls `render_mapping_row` directly with `joy_edits[i]`,
  `joy_label_edits[i]`, `joy_repeat_edits[i]`, and the `Up`/`Down`/`Left`/`Right` name.

## Scope

- **Purely visual.** No change to behavior, config, persistence, validity logic, or
  the Save handler. The label hint is "label (optional)" everywhere.
- GUI code — verified manually (a glance at the Bindings tab), no unit test.

## Out of scope

- Any change to what a row edits or how it saves.
- Column alignment/grid beyond the existing horizontal layout.
