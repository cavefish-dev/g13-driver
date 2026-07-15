# Per-binding labels — design

- **Status:** approved (design)
- **Date:** 2026-07-15
- **Scope:** Add an optional human-readable label to each G-key binding (e.g. `G1 = "ctrl+c"` →
  label "Copy"), stored in a parallel `[labels]` table, editable on the Bindings tab, and shown on
  the Monitor grid. Makes downloaded/catalog profiles self-explanatory and lays the data groundwork
  for the future LCD per-key hints (LCD itself is out of scope — the unbuilt v0.4 milestone).

## Motivation

Profiles list bindings as raw combos (`ctrl+c`), which don't say what the key *does*. Labels let a
curated catalog profile explain itself ("Copy", "Toggle comment") and give the future LCD work a
per-key hint string to display. Labels are an optional overlay — nothing about existing profiles or
the injection path changes.

## Decisions (from brainstorming)

- **Storage:** a parallel `[labels]` table keyed by G-key, mirroring the existing `[repeat]` table
  (not a nested per-key structure, which would break the flat `[keys]` format and every profile).
- **Surfacing:** editable on the Bindings tab (a label field per row) **and** shown read-only on the
  Monitor grid (stacked in each cell, cells grow taller). LCD display deferred.
- **Monitor cell:** three-line stack — key name · combo · label — with uniform cell height.
- **Provenance:** editing a label saves through the same path as combos/repeat, so it flips
  `modified = true` on a GitHub profile (a label change is a real divergence from upstream).
- **Shipped profiles get labels** (bundled `basic`/`media` + catalog `gaming`/`coding`) to demo the
  feature and deliver the "understand a downloaded profile" value out of the box.

## Schema: the `[labels]` table

```toml
[keys]
G1 = "ctrl+c"

[repeat]
G2 = true

[labels]
G1 = "Copy"
```

- `RawConfig` gains `#[serde(default)] labels: HashMap<String, String>` (string keys parsed via the
  existing `parse_g13_key`, exactly like `[keys]`/`[repeat]`). An unknown key in `[labels]` is an
  error, consistent with `[keys]`/`[repeat]`.
- `Profile` gains `labels: HashMap<G13Key, String>`, populated in `from_raw` (values trimmed;
  empty ⇒ omitted). Accessors: `label(&self, G13Key) -> Option<&str>` and
  `set_labels(&mut self, HashMap<G13Key, String>)`, mirroring `repeats()`/`set_repeat()`.
- `to_toml()` writes a `[labels]` table containing only keys with a non-empty label (sparse, like
  `[repeat]` omits `false`); a profile with no labels emits no `[labels]` table.
- Backward-compatible: existing profiles have no `[labels]` and load unchanged. Labels are an
  independent overlay — a `[labels]` entry for a key with no `[keys]` binding still loads (the label
  is simply unused until the key is bound).

## Bindings tab: editing (`render_binding_row`)

- Each row gains a **label** text field after the `repeat` checkbox: `key · combo field · repeat ☐ ·
  label field`. Free text, no validation (any string; empty = no label). Placeholder *"label
  (optional)"*.
- New edit buffer `label_edits: HashMap<G13Key, String>` on `MonitorApp`, populated from the active
  profile's labels when the binding buffers reload (alongside `edits`/`repeat_edits`), and written
  on **Save**.
- `ProfileSet::save_active_bindings` is extended to `save_active_bindings(bindings, repeat, labels)`;
  it sets the labels on the active profile before serializing. Saving label changes on a GitHub
  profile flips `modified = true` via the existing path (no special-casing).
- The label field takes the remaining row width to the right of the combo field / repeat checkbox.

## Monitor grid: display (`render_monitor`)

Each cell becomes a three-line vertical stack inside the existing 48px-wide frame:

```
G1        ← key name (ui.strong)
ctrl+c    ← combo (small)
Copy      ← label (small, .weak() dimmed)
```

- Add a third `Label` for `cfg.label(key)`, rendered `.weak()` and `.truncate()`.
- **Uniform height:** reserve the label line's height even when a key has no label (render the label
  or an empty small line), so every cell in a row is the same height and the grid stays aligned.
  Cells auto-grow taller to fit the extra line.
- The `CELL`/`BLOCK_W` width constants and the joystick panel are unchanged — only cell height grows.

## Shipped/catalog profile labels

- `catalog/gaming.toml`, `catalog/coding.toml`: add a `[labels]` entry for each bound key
  (human-readable action names). The CI index generator is unaffected (it reads only `[meta].name`).
- `profiles/basic.toml`, `profiles/media.toml`: add labels too, so the built-in profiles demonstrate
  the feature out of the box.

## Error handling

Consistent with project policy: an unknown G-key in `[labels]` errors on load (like `[keys]`); label
parsing/serialization is infallible string handling; no `panic!`/`unwrap()` on profile data. Empty
labels are dropped, not written. A label edit failing to save surfaces on the Bindings status line
as today.

## Testing

- **Unit (TDD, pure logic):**
  - `[labels]` parse → `Profile::label(key)` (present / absent / empty→None); unknown key in
    `[labels]` errors; a `[labels]` entry without a matching `[keys]` binding still loads.
  - `to_toml()` round-trips labels and OMITS the `[labels]` table when there are none.
  - `save_active_bindings(bindings, repeat, labels)` writes the `[labels]` table; on a GitHub profile
    it flips `modified = true`; on a user profile the file stays clean of provenance lines.
- **Manual-verify (GUI, documented exception):** the Bindings row label field edits + saves; the
  Monitor grid shows the third label line with uniform cell height; a labeled catalog profile reads
  clearly.

## Out of scope (follow-ups)

- LCD per-key hint display (the v0.4 LCD milestone consumes `Profile::label(key)`).
- Label localization / i18n; per-label styling.
- Auto-suggesting labels from the combo (e.g. inferring "Copy" from `ctrl+c`).
