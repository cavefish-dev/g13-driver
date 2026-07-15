# Per-binding labels

- **Status:** finished — GUI smoke-tested (2026-07-15)
- **Date:** 2026-07-15

## Outcome
Each G-key binding can carry an optional human-readable label. Spec:
`docs/superpowers/specs/2026-07-15-binding-labels-design.md`; plan:
`docs/superpowers/plans/2026-07-15-binding-labels.md`.

- **Schema:** a `[labels]` TOML table parallel to `[repeat]` — `Profile.labels: HashMap<G13Key,
  String>` with `label(key) -> Option<&str>` / `set_labels(map)`. Sparse (only non-empty written);
  `to_toml` omits the table when empty; an unknown G-key in `[labels]` errors on load; empty/
  whitespace ⇒ dropped. Backward-compatible — profiles without `[labels]` load unchanged, and a
  `[labels]` entry without a matching `[keys]` binding still loads.
- **Editing:** the Bindings tab gains a per-row label field (`label_edits` buffer);
  `save_active_bindings` grew a `labels` arg and persists them. Editing a label on a GitHub profile
  flips `modified` (a label change is a real divergence), via the existing save path.
- **Display:** the Monitor grid cell is now a three-line stack — key · combo · label (dimmed,
  `.small().weak()`), always rendered (blank when absent) so rows stay aligned; cells grow taller.
- **Shipped profiles labeled:** `basic`, `media`, `catalog/gaming`, `catalog/coding` each got a
  `[labels]` table (keys matching their bindings) so the built-ins/catalog are self-explanatory.

Built via subagent-driven-development (5 tasks), 168 unit tests. Task 1 full review + final
whole-branch review (opus): MERGE, no Critical/Important — schema round-trips (a label-less profile
never emits `[labels]`), a label edit correctly flips `modified`, all `save_active_bindings` /
`render_binding_row` call sites updated.

## Smoke test — PASSED 2026-07-15
Verified live: the Monitor grid shows the third label line (e.g. G1 `ctrl+c` / "Copy") with uniform
cell height; the Bindings tab label field is pre-filled and edits/saves; switching profiles updates
the Monitor labels.

## Notes / follow-ups
- The catalog `index.json` is unaffected (labels don't touch `[meta].name`); no CI regeneration
  needed.
- **LCD (deferred, v0.4):** the LCD milestone consumes `Profile::label(key)` as a per-key hint —
  the data now exists.
- Deferred (spec out-of-scope): label inference from the combo, i18n / per-label styling.
- Minor (final review, non-blocking): a harmless redundant re-trim in the `to_toml` labels builder.
