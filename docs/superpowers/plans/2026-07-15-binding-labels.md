# Per-binding Labels Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give each G-key binding an optional human-readable label (stored in a parallel `[labels]` table), editable on the Bindings tab and shown on the Monitor grid.

**Architecture:** A `[labels]` TOML table parallels the existing `[repeat]` table; `Profile` gains a `labels: HashMap<G13Key, String>` map with parse/serialize/accessors mirroring `repeat`. `save_active_bindings` grows a `labels` arg. The Bindings row gets a label text field; the Monitor cell gets a third stacked label line. Shipped profiles get labels.

**Tech Stack:** Rust (GNU toolchain), eframe/egui 0.31, `serde`/`toml`.

## Global Constraints

- GNU toolchain only; if `cargo`/`gcc` missing: `export PATH="$HOME/.cargo/bin:/c/Strawberry/c/bin:$PATH"`. Run `cargo test` (never `--lib`).
- **`[labels]` mirrors `[repeat]`:** parallel table keyed by G-key; optional and **sparse** (only non-empty labels written); an unknown G-key in `[labels]` is a load error (like `[keys]`/`[repeat]`); empty/whitespace label ⇒ omitted. Backward-compatible: profiles without `[labels]` load unchanged. A `[labels]` entry for a key with no `[keys]` binding still loads.
- **Provenance:** editing a label saves through `save_active_bindings`, which already flips `modified = true` on a GitHub profile — no special-casing.
- No `panic!`/`unwrap()`/`expect()` on profile data (tests may unwrap; the existing `active_profile_mut().expect("checked above")` in `save_active_bindings` stays).
- GUI code (Bindings row field, Monitor cell) is manual-verify (documented exception).
- Branch `feat/binding-labels` off `main`. Commit trailer: `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.

---

### Task 1: `[labels]` schema

**Files:**
- Modify: `src/config.rs` — `RawConfig`, `Profile` field + accessors, `from_raw`, `to_toml`, `Default`, the `raw()` test helper.
- Test: `src/config.rs` (`#[cfg(test)] mod tests`).

**Interfaces:**
- Produces: `Profile::label(&self, G13Key) -> Option<&str>`, `Profile::set_labels(&mut self, HashMap<G13Key, String>)`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn parses_labels() {
    let src = "[keys]\nG1 = \"ctrl+c\"\n[labels]\nG1 = \"Copy\"\n";
    let p = Profile::from_raw(toml::from_str(src).unwrap()).unwrap();
    assert_eq!(p.label(G13Key::G1), Some("Copy"));
    assert_eq!(p.label(G13Key::G2), None);
}

#[test]
fn empty_label_is_omitted() {
    let src = "[keys]\nG1 = \"a\"\n[labels]\nG1 = \"  \"\n";
    let p = Profile::from_raw(toml::from_str(src).unwrap()).unwrap();
    assert_eq!(p.label(G13Key::G1), None);
}

#[test]
fn unknown_key_in_labels_errors() {
    let src = "[labels]\nG99 = \"Nope\"\n";
    assert!(Profile::from_raw(toml::from_str(src).unwrap()).is_err());
}

#[test]
fn label_without_binding_still_loads() {
    let src = "[labels]\nG1 = \"Copy\"\n"; // no [keys]
    let p = Profile::from_raw(toml::from_str(src).unwrap()).unwrap();
    assert_eq!(p.label(G13Key::G1), Some("Copy"));
    assert_eq!(p.get_binding(G13Key::G1), None);
}

#[test]
fn to_toml_round_trips_labels() {
    let mut p = Profile::from_raw(raw(&[("G1", "ctrl+c")])).unwrap();
    let mut labels = HashMap::new();
    labels.insert(G13Key::G1, "Copy".to_string());
    p.set_labels(labels);
    let toml = p.to_toml().unwrap();
    assert!(toml.contains("[labels]"));
    let reloaded = Profile::from_raw(toml::from_str(&toml).unwrap()).unwrap();
    assert_eq!(reloaded.label(G13Key::G1), Some("Copy"));
}

#[test]
fn to_toml_omits_empty_labels_table() {
    let p = Profile::from_raw(raw(&[("G1", "a")])).unwrap();
    assert!(!p.to_toml().unwrap().contains("[labels]"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test parses_labels empty_label_is_omitted unknown_key_in_labels label_without_binding to_toml_round_trips_labels to_toml_omits_empty_labels`
Expected: FAIL — `no method named label`.

- [ ] **Step 3: Implement**

In `RawConfig` (after the `repeat` field), add:

```rust
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub labels: HashMap<String, String>,
```

Add the field to `Profile` (after `repeat: HashMap<G13Key, bool>,`):

```rust
    labels: HashMap<G13Key, String>,
```

In `from_raw`, after the `repeat` parse loop, add a labels parse loop:

```rust
        let mut labels = HashMap::new();
        for (name, text) in raw.labels {
            let key = parse_g13_key(&name)
                .with_context(|| format!("unknown G13 key in [labels]: {}", name))?;
            let text = text.trim().to_string();
            if !text.is_empty() { labels.insert(key, text); }
        }
```

Add `labels` to the returned `Ok(Self { … })` (append `, labels`).

Add accessors (near `repeats`/`set_repeat`):

```rust
    pub fn label(&self, key: G13Key) -> Option<&str> {
        self.labels.get(&key).map(|s| s.as_str())
    }

    pub fn set_labels(&mut self, labels: HashMap<G13Key, String>) {
        self.labels = labels.into_iter()
            .map(|(k, v)| (k, v.trim().to_string()))
            .filter(|(_, v)| !v.is_empty())
            .collect();
    }
```

In `to_toml`, after the `repeat` builder and before `let meta = { … }`, add the labels builder, and include `labels` in the `RawConfig { … }` literal:

```rust
        let labels: HashMap<String, String> = self.labels.iter()
            .filter(|(_, v)| !v.trim().is_empty())
            .map(|(k, v)| (format!("{k:?}"), v.clone()))
            .collect();
        // … meta builder unchanged …
        let raw = RawConfig { meta, keys, joystick, repeat, labels };
```

In `Default for Profile`, add `labels: HashMap::new(),`.

In the `raw()` test helper, add `labels: HashMap::new(),`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test`
Expected: PASS — all prior + the 6 new. (Fix any other `RawConfig { … }` literal the compiler flags by adding `labels: HashMap::new(),` — the `to_toml` builder and the `raw()` helper are the only two.)

- [ ] **Step 5: Commit**

```bash
git add src/config.rs
git commit -m "feat: add [labels] schema — per-binding display names"
```

---

### Task 2: `save_active_bindings` writes labels

**Files:**
- Modify: `src/config.rs` — `save_active_bindings` signature + body; update its existing tests; the one caller in `src/monitor/mod.rs` (temporary empty map).
- Test: `src/config.rs` (`#[cfg(test)] mod profileset_tests`).

**Interfaces:**
- Consumes: `Profile::set_labels` (Task 1).
- Produces: `ProfileSet::save_active_bindings(bindings, repeat, labels: HashMap<G13Key, String>) -> Result<()>`.

- [ ] **Step 1: Write the failing test** (in `mod profileset_tests`)

```rust
    #[test]
    fn save_active_bindings_writes_labels() {
        let d = tmp("save-labels");
        write(&d.join("profiles"), "p.toml", "[keys]\nG1 = \"a\"\n");
        write(&d, "config.toml", "profiles_dir = \"profiles\"\nm1 = \"p.toml\"\n");
        let mut set = ProfileSet::load(&d.join("config.toml")).unwrap();
        let mut b = HashMap::new();
        b.insert(G13Key::G1, "ctrl+c".to_string());
        let mut labels = HashMap::new();
        labels.insert(G13Key::G1, "Copy".to_string());
        set.save_active_bindings(b, HashMap::new(), labels).unwrap();
        let text = std::fs::read_to_string(d.join("profiles/p.toml")).unwrap();
        assert!(text.contains("[labels]"));
        assert!(text.contains("Copy"));
        // reloads with the label intact
        let reloaded = ProfileSet::load(&d.join("config.toml")).unwrap();
        assert_eq!(reloaded.active_profile().unwrap().label(G13Key::G1), Some("Copy"));
    }
```

Also update the THREE existing calls to `save_active_bindings` in `mod profileset_tests`
(`save_active_bindings_writes_and_preserves_others`, `saving_github_profile_marks_modified`,
`saving_user_profile_stays_clean`) to pass a third argument `HashMap::new()`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test save_active_bindings_writes_labels`
Expected: FAIL — arity mismatch / no `[labels]` written. (The existing tests won't compile until updated — update them in Step 1.)

- [ ] **Step 3: Implement**

Extend the signature and set the labels before serializing (in `src/config.rs`):

```rust
    pub fn save_active_bindings(
        &mut self,
        bindings: HashMap<G13Key, String>,
        repeat: HashMap<G13Key, bool>,
        labels: HashMap<G13Key, String>,
    ) -> Result<()> {
        if self.active_name().is_none() || self.active_profile().is_none() {
            anyhow::bail!("no profile in the active slot");
        }
        let path = self.active_path();
        let profile = self.active_profile_mut().expect("checked above");
        profile.set_bindings(bindings);
        profile.set_repeat(repeat);
        profile.set_labels(labels);
        if profile.source() == ProfileSource::Github {
            profile.set_modified(true);
        }
        let toml = profile.to_toml()?;
        std::fs::write(&path, toml)
            .with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }
```

Update the single caller in `src/monitor/mod.rs` (the Save button in `render_bindings`) to pass an
empty labels map for now — Task 3 replaces it with the real buffer:

```rust
                    match self.profiles.write().unwrap().save_active_bindings(bindings, repeat, HashMap::new()) {
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test`
Expected: PASS — including `save_active_bindings_writes_labels`; `cargo build` compiles (the monitor caller updated).

- [ ] **Step 5: Commit**

```bash
git add src/config.rs src/monitor/mod.rs
git commit -m "feat: save_active_bindings writes the [labels] table"
```

---

### Task 3: Bindings tab — label field per row

GUI — **manual-verify, no unit tests**.

**Files:**
- Modify: `src/monitor/mod.rs` — `MonitorApp` field `label_edits`; the buffer-reload block; `render_binding_row`; the Save button.

**Interfaces:**
- Consumes: `Profile::label` (Task 1), `save_active_bindings(..., labels)` (Task 2).

- [ ] **Step 1: Add the buffer + reload it**

Add a field to `struct MonitorApp` (near `repeat_edits`):

```rust
    label_edits: HashMap<G13Key, String>,
```

Initialize it in `MonitorApp::new` (near the other edit buffers): `label_edits: HashMap::new(),`.

In `render_bindings`, in the buffer-reload block (where `self.edits` and `self.repeat_edits` are
rebuilt when `self.edits_for != active_name`), add the label buffer alongside them:

```rust
                self.label_edits = ROWS.iter().flat_map(|row| row.iter()).chain(THUMB.iter())
                    .map(|&k| (k, profile.label(k).unwrap_or_default().to_string()))
                    .collect();
```

- [ ] **Step 2: Add the label field to `render_binding_row`**

Add a parameter and a text field. New signature + body tail:

```rust
fn render_binding_row(
    ui: &mut egui::Ui,
    key: G13Key,
    edits: &mut HashMap<G13Key, String>,
    repeat_edits: &mut HashMap<G13Key, bool>,
    label_edits: &mut HashMap<G13Key, String>,
    valid_keys: &HashSet<String>,
) {
    let green = egui::Color32::from_rgb(127, 224, 160);
    let red = egui::Color32::from_rgb(220, 90, 90);
    let dim = egui::Color32::from_gray(110);
    let buf = edits.entry(key).or_default();
    let rep = repeat_edits.entry(key).or_default();
    let lbl = label_edits.entry(key).or_default();
    ui.horizontal(|ui| {
        ui.monospace(format!("{key:?}"));
        ui.add_space(6.0);
        ui.add(egui::TextEdit::singleline(buf).desired_width(160.0));
        let (mark, color) = if buf.is_empty() {
            ("—", dim)
        } else if combo_valid(buf, valid_keys) {
            ("ok", green)
        } else {
            ("bad", red)
        };
        ui.colored_label(color, mark);
        ui.add_space(6.0);
        ui.checkbox(rep, "repeat");
        ui.add_space(6.0);
        ui.add(egui::TextEdit::singleline(lbl).desired_width(140.0).hint_text("label (optional)"));
    });
}
```

Note the borrow: `lbl` is borrowed from `label_edits` before the closure and moved in — mirror how
`buf`/`rep` are already handled (they're `entry().or_default()` before `ui.horizontal`). This
compiles because each is a distinct `&mut` into a different map.

Update BOTH `render_binding_row(...)` call sites in `render_bindings` (the ROWS loop and the THUMB
loop) to pass `&mut self.label_edits`:

```rust
                render_binding_row(ui, key, &mut self.edits, &mut self.repeat_edits, &mut self.label_edits, &valid_keys);
```

- [ ] **Step 3: Pass labels to Save**

In the Save button handler, build the labels map from the buffer (drop empties) and pass it:

```rust
                let labels: HashMap<G13Key, String> = self.label_edits.iter()
                    .filter(|(_, v)| !v.trim().is_empty())
                    .map(|(k, v)| (*k, v.clone()))
                    .collect();
                match self.profiles.write().unwrap().save_active_bindings(bindings, repeat, labels) {
```

- [ ] **Step 4: Build + manual smoke**

Run: `cargo build` then `cargo test` (unchanged count — no new unit tests).

Manual (record for the milestone): a label field appears on each Bindings row; typing a label and
Save persists it (the profile file gains a `[labels]` entry); editing a label on a GitHub profile
flips its badge to "GitHub · edited".

- [ ] **Step 5: Commit**

```bash
git add src/monitor/mod.rs
git commit -m "feat: Bindings tab — per-row label field"
```

---

### Task 4: Monitor grid — stacked label line

GUI — **manual-verify, no unit tests**.

**Files:**
- Modify: `src/monitor/mod.rs` — the cell rendering in `render_monitor`.

- [ ] **Step 1: Add the third stacked line**

In `render_monitor`, inside the per-key cell `Frame`, the `ui.vertical(|ui| { … })` currently renders
the key name (`ui.strong`) and the combo (small, truncated). Add a third line for the label — always
rendered (blank when absent) so every cell in a row is the same height:

```rust
                            egui::Frame::new().fill(fill).inner_margin(4.0).outer_margin(3.0).corner_radius(4.0).show(ui, |ui| {
                                ui.set_width(48.0);
                                ui.vertical(|ui| {
                                    ui.strong(format!("{key:?}"));
                                    ui.add(egui::Label::new(egui::RichText::new(binding).small()).truncate());
                                    let label = cfg.and_then(|c| c.label(key)).unwrap_or("");
                                    ui.add(egui::Label::new(egui::RichText::new(label).small().weak()).truncate());
                                });
                            });
```

(The empty-string label still allocates a small text line, keeping all cells the same taller height.)

- [ ] **Step 2: Build + manual smoke**

Run: `cargo build` then `cargo test` (unchanged count).

Manual (record for the milestone): the Monitor grid cells are taller and show a third dimmed line
with the label; keys without a label show a blank third line and the row stays aligned; a labeled
profile (e.g. Media after Task 5) reads clearly (e.g. G1 shows `space` / `Play/Pause`).

- [ ] **Step 3: Commit**

```bash
git add src/monitor/mod.rs
git commit -m "feat: Monitor grid shows per-key label under the combo"
```

---

### Task 5: Label the shipped & catalog profiles

**Files:**
- Modify: `profiles/basic.toml`, `profiles/media.toml`, `catalog/gaming.toml`, `catalog/coding.toml`.

Add a `[labels]` table (after the existing `[keys]`/`[joystick]`) to each, labeling each bound key.
Read each file first to match its actual keys; label every key present in `[keys]`.

- [ ] **Step 1: `profiles/media.toml`** — append:

```toml
[labels]
G1 = "Play/Pause"
G2 = "Previous"
G3 = "Next"
G4 = "Volume Up"
G5 = "Volume Down"
```

(Match to media.toml's actual keys — its current keys are G1=space, G2=left, G3=right, G4=up,
G5=down; label them by intent as above if they map to media transport, otherwise by their action.)

- [ ] **Step 2: `profiles/basic.toml`** — append a `[labels]` table labeling its keys, e.g.:

```toml
[labels]
G1 = "Copy"
G2 = "Paste"
G3 = "Undo"
G4 = "Redo"
G5 = "Refresh"
G6 = "Switch Window"
G7 = "Show Desktop"
G8 = "Cut"
G9 = "Save"
G10 = "Select All"
G11 = "Find"
G12 = "Close"
```

- [ ] **Step 3: `catalog/gaming.toml`** — append labels for its keys (G1–G7 number/action row):

```toml
[labels]
G1 = "Slot 1"
G2 = "Slot 2"
G3 = "Slot 3"
G4 = "Slot 4"
G5 = "Reload"
G6 = "Use"
G7 = "Jump"
```

- [ ] **Step 4: `catalog/coding.toml`** — append labels for its keys (match the final key set,
including any substitutions made when the catalog was seeded), e.g.:

```toml
[labels]
G1 = "Copy"
G2 = "Paste"
G3 = "Comment Line"
G4 = "Find"
G5 = "Rename"
G6 = "Go to Definition"
G7 = "Find in Files"
G8 = "Toggle Sidebar"
G9 = "New Terminal"
```

- [ ] **Step 5: Verify all four load + labels visible**

Run: `cargo test` (config tests unaffected — they use temp files; must stay green). Then confirm each
edited profile still parses (a label referencing a key that exists is fine; a `[labels]` key must be
a valid G-key name or load fails). If unsure, load-check each with a throwaway
`Profile::load(Path::new("catalog/gaming.toml")).unwrap()` test, run, then remove it.

- [ ] **Step 6: Commit**

```bash
git add profiles/basic.toml profiles/media.toml catalog/gaming.toml catalog/coding.toml
git commit -m "feat: add labels to bundled + catalog profiles"
```

---

## Notes for the executor

- `cargo test` (never `--lib`). Tasks 1–2 are unit-tested; Tasks 3–4 are GUI manual-verify; Task 5 is content (verify each profile still loads).
- After all tasks: final whole-branch review, then `superpowers:finishing-a-development-branch`. The manual smoke items (Tasks 3–4) + a look at the labeled profiles become the milestone smoke-test checklist. Before a GUI smoke test, sync the beside-exe bundle (`target/release/config.toml` + `profiles/`) to the repo.
- The catalog `index.json` is unaffected by labels (it reads only `[meta].name`), so no CI regeneration is needed for this change.
