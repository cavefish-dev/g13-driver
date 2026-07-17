# Unify Bindings-tab Row Style — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One shared `render_mapping_row` (joystick style) for all Bindings-tab rows — G-keys, thumb buttons, and joystick directions.

**Architecture:** Extract the joystick-style row into `render_mapping_row`; make the existing `render_binding_row` a thin wrapper over it (HashMap entries + `{key:?}` name); rewire the joystick loop to call it directly.

**Tech Stack:** Rust, egui. GUI-only, no behavior change.

## Global Constraints
- **GNU toolchain only.** Build/test with `stable-x86_64-pc-windows-gnu`; MinGW gcc at `C:\Strawberry\c\bin\gcc.exe`. If `cargo`/`gcc` not found, prepend to PATH per CLAUDE.md. Do NOT switch to the MSVC target.
- **Purely visual** — no change to what a row edits, validity logic, persistence, or the Save handler. Label hint is **"label (optional)"** everywhere.
- GUI code — no unit test (manual verify). Verification = clean `cargo build` + existing `cargo test` green (208).
- One commit; imperative subject.

---

## Task 1: Extract `render_mapping_row` and rewire all rows

**Files:**
- Modify: `src/monitor/mod.rs` (`render_binding_row` ~line 79; joystick loop ~line 1205)

- [ ] **Step 1: Add `render_mapping_row` and make `render_binding_row` a wrapper**

Replace the existing `render_binding_row` (currently ~lines 79-110) with the shared renderer + a thin wrapper:

```rust
/// One Bindings-tab mapping row (shared by G-keys, thumb buttons, joystick):
/// right-aligned name, key/combo field, validity mark, label field, repeat checkbox.
fn render_mapping_row(
    ui: &mut egui::Ui,
    name: &str,
    key_buf: &mut String,
    label_buf: &mut String,
    repeat_buf: &mut bool,
    valid_keys: &HashSet<String>,
) {
    let green = egui::Color32::from_rgb(127, 224, 160);
    let red = egui::Color32::from_rgb(220, 90, 90);
    let dim = egui::Color32::from_gray(110);
    ui.horizontal(|ui| {
        ui.monospace(format!("{name:>5}"));
        ui.add_space(6.0);
        ui.add(egui::TextEdit::singleline(key_buf).desired_width(160.0));
        let (mark, color) = if key_buf.is_empty() {
            ("—", dim)
        } else if combo_valid(key_buf, valid_keys) {
            ("ok", green)
        } else {
            ("bad", red)
        };
        ui.colored_label(color, mark);
        ui.add(egui::TextEdit::singleline(label_buf)
            .desired_width(140.0).hint_text("label (optional)"));
        ui.checkbox(repeat_buf, "repeat");
    });
}

fn render_binding_row(
    ui: &mut egui::Ui,
    key: G13Key,
    edits: &mut HashMap<G13Key, String>,
    repeat_edits: &mut HashMap<G13Key, bool>,
    label_edits: &mut HashMap<G13Key, String>,
    valid_keys: &HashSet<String>,
) {
    let name = format!("{key:?}");
    // Separate HashMaps → three independent &mut borrows, no aliasing.
    let key_buf = edits.entry(key).or_default();
    let label_buf = label_edits.entry(key).or_default();
    let repeat_buf = repeat_edits.entry(key).or_default();
    render_mapping_row(ui, &name, key_buf, label_buf, repeat_buf, valid_keys);
}
```

- [ ] **Step 2: Rewire the joystick loop to `render_mapping_row`**

Replace the inline joystick row loop (currently ~lines 1205-1218, the `for (i, name) in ["Up","Down","Left","Right"]...` block AND the three `let dim/green/red` color lines just above it that only the inline row used) with:

```rust
            ui.label("Joystick");
            for (i, name) in ["Up", "Down", "Left", "Right"].into_iter().enumerate() {
                render_mapping_row(ui, name, &mut self.joy_edits[i], &mut self.joy_label_edits[i],
                    &mut self.joy_repeat_edits[i], &valid_keys);
            }
```

(Use `.into_iter()` so `name` is `&str`. The `&mut self.joy_edits[i]` / `joy_label_edits[i]` / `joy_repeat_edits[i]` are borrows of three DIFFERENT fields, so no aliasing conflict. Remove the now-unused `let dim = ...; let green = ...; let red = ...;` lines that preceded the old inline loop — `render_mapping_row` defines its own — to keep the build warning-clean.)

- [ ] **Step 3: Build + test**

Run: `cargo build` then `cargo test`
Expected: clean build (no unused-variable warnings from the removed color lines); 208 tests pass.

- [ ] **Step 4: Manual check**

Run: `cargo run` → Bindings tab. Confirm G-key, thumb, and joystick rows are now identical in style: right-aligned name → key field → validity mark → label ("label (optional)") → repeat checkbox. Confirm editing + Save still work.

- [ ] **Step 5: Commit**

```bash
git add src/monitor/mod.rs
git commit -m "refactor(gui): unify Bindings-tab rows on one joystick-style renderer"
```

---

## Task 2: Milestone

**Files:**
- Move: `milestones/open/unify-binding-rows.md` → `milestones/ongoing/`

- [ ] **Step 1: Update + move**

Edit `milestones/open/unify-binding-rows.md`: set `Status: ongoing`, check the task box, add a one-line smoke note ("all Bindings rows visually identical; edit+save unchanged — confirm in GUI"). Then `git mv milestones/open/unify-binding-rows.md milestones/ongoing/unify-binding-rows.md`.

- [ ] **Step 2: Full build + test**

Run: `cargo test && cargo build --release`
Expected: all pass; release builds clean.

- [ ] **Step 3: Commit**

```bash
git add milestones/
git commit -m "docs: unify-binding-rows milestone to ongoing"
```

---

## Self-Review
- Shared renderer used by G-keys/thumb (via wrapper) + joystick → Task 1. ✓
- Joystick style (right-aligned name, label before repeat), hint "label (optional)" → Task 1. ✓
- No behavior/persistence change; Save handler untouched → Task 1 (only rendering changed). ✓
- Placeholder scan: complete code. Borrow safety noted (separate HashMaps / separate fields). Types: `render_mapping_row(ui, &str, &mut String, &mut String, &mut bool, &HashSet<String>)` consistent across both call paths.
