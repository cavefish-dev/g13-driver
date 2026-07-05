# Thumb Buttons Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the two near-joystick buttons and the joystick click bindable like G-keys and show them in the GUI.

**Architecture:** Add three `G13Key` variants (`Btn1`, `Btn2`, `Stick`); the parser decodes byte 7 bits 1/2/3 into ordinary `KeyDown`/`KeyUp(G13Key)` events, so the dispatcher (hold-means-hold), `DeviceState`, `Config`, and the editor all pick them up unchanged. The GUI gains a thumb-button editor section and a monitor indicator.

**Tech Stack:** Rust, GNU toolchain (`stable-x86_64-pc-windows-gnu`), `eframe`/`egui`, `toml`/`serde`, `log`. Build/test: `cargo` (PATH may need `export PATH="$HOME/.cargo/bin:/c/Strawberry/c/bin:$PATH"`).

## Global Constraints

- **Windows-only** (`src/main.rs:1-2`). `protocol`/`config`/`monitor` stay platform-neutral.
- **TDD** for pure logic (protocol decode, config parse/serialize). GUI rendering is manual-verify (documented exception) — verified by the hardware smoke test.
- **Hardware-verified bits (byte 7):** `Btn1` = bit 1 (`0x02`), `Btn2` = bit 2 (`0x04`), `Stick` (joystick click) = bit 3 (`0x08`). Bit 0 (MR) and bit 7 (heartbeat) are NOT touched by the thumb decode (the M-key decode still owns bit 0).
- **Config names (case-insensitive):** `BTN1`→`Btn1`, `BTN2`→`Btn2`, `STICK`→`Stick`. `to_toml` serializes via the variant's Debug name (`Btn1`/`Btn2`/`Stick`), read back uppercased.
- **No dispatcher/device_state change** — thumb buttons are ordinary `KeyDown`/`KeyUp(G13Key)` events; they hold-means-hold (or tap a media key) via the existing dispatch.
- **Error policy:** no `panic!`/`unwrap()` in the runtime path (test code / `mutex.lock().unwrap()` excepted).
- **Commits:** one per task; imperative subject; end with the trailer `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.
- Binary crate — `cargo test` (not `--lib`); focused: `cargo test <module>::`.

---

## File Structure

| File | Change | Responsibility |
|------|--------|----------------|
| `src/protocol.rs` | Modify | `G13Key::{Btn1,Btn2,Stick}`; parser decodes byte 7 bits 1/2/3 |
| `src/config.rs` | Modify | `parse_g13_key` names for the three buttons |
| `src/monitor/mod.rs` | Modify | Editor "Thumb buttons" section; monitor indicator row |

---

## Task 1: Decode the thumb buttons

**Files:** Modify `src/protocol.rs`.

**Interfaces:**
- Produces: `G13Key::Btn1`, `G13Key::Btn2`, `G13Key::Stick`; parser emits `KeyDown`/`KeyUp` for them from byte 7 bits 1/2/3.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `src/protocol.rs` (the `idle()` helper — `[0x01,0x7F,0x7F,0,0,0x80,0,0]` — already exists; byte 7 = 0 at idle):

```rust
    #[test]
    fn thumb_btn1_press_and_release() {
        let mut p = ReportParser::new();
        let mut r = idle();
        r[7] = 0x02; // Btn1 = byte 7 bit 1
        assert_eq!(p.parse(&r), vec![G13Event::KeyDown(G13Key::Btn1)]);
        assert_eq!(p.parse(&idle()), vec![G13Event::KeyUp(G13Key::Btn1)]);
    }

    #[test]
    fn thumb_btn2_and_stick() {
        let mut p = ReportParser::new();
        let mut r = idle();
        r[7] = 0x04; // Btn2 = bit 2
        assert_eq!(p.parse(&r), vec![G13Event::KeyDown(G13Key::Btn2)]);
        let mut r = idle();
        r[7] = 0x08; // Stick (joystick click) = bit 3
        let ev = p.parse(&r);
        assert!(ev.contains(&G13Event::KeyUp(G13Key::Btn2)));
        assert!(ev.contains(&G13Event::KeyDown(G13Key::Stick)));
    }

    #[test]
    fn thumb_ignores_mr_and_heartbeat() {
        let mut p = ReportParser::new();
        let mut r = idle();
        r[7] = 0x81; // bit 0 = MR (an M-key, not a thumb button) + bit 7 heartbeat
        // No thumb KeyDown/KeyUp should be emitted (MR is handled by the M-key decode).
        let ev = p.parse(&r);
        assert!(!ev.iter().any(|e| matches!(e,
            G13Event::KeyDown(G13Key::Btn1 | G13Key::Btn2 | G13Key::Stick)
            | G13Event::KeyUp(G13Key::Btn1 | G13Key::Btn2 | G13Key::Stick))));
    }

    #[test]
    fn thumb_and_gkey_together() {
        let mut p = ReportParser::new();
        let mut r = idle();
        r[3] = 0b0000_0001; // G1
        r[7] = 0x02;        // Btn1
        let ev = p.parse(&r);
        assert!(ev.contains(&G13Event::KeyDown(G13Key::G1)));
        assert!(ev.contains(&G13Event::KeyDown(G13Key::Btn1)));
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test protocol:: 2>&1 | tail -15`
Expected: FAIL — `no variant Btn1 found for enum G13Key`.

- [ ] **Step 3: Add the variants + decode**

In `src/protocol.rs`, extend the `G13Key` enum:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum G13Key {
    G1,  G2,  G3,  G4,  G5,  G6,  G7,  G8,
    G9,  G10, G11, G12, G13, G14, G15, G16,
    G17, G18, G19, G20, G21, G22,
    // Thumb inputs (byte 7): the two buttons next to the joystick + the stick click.
    Btn1, Btn2, Stick,
}
```

Add `prev_buttons: u8` to `ReportParser` and init it:

```rust
pub struct ReportParser {
    prev_keys: u32,
    prev_x: u8,
    prev_y: u8,
    prev_mkeys: u8,
    prev_buttons: u8,
}

impl ReportParser {
    pub fn new() -> Self {
        Self { prev_keys: 0, prev_x: 127, prev_y: 127, prev_mkeys: 0, prev_buttons: 0 }
    }
```

In `parse`, after the M-key block and before `events`, add the thumb decode:

```rust
        // Thumb buttons: byte 7 bit 1 = Btn1, bit 2 = Btn2, bit 3 = Stick (joystick
        // click). Ordinary KeyDown/KeyUp so they reuse the G-key binding path.
        let current_b = (u8::from(report[7] & 0x02 != 0))
            | (u8::from(report[7] & 0x04 != 0) << 1)
            | (u8::from(report[7] & 0x08 != 0) << 2);
        let b_pressed = current_b & !self.prev_buttons;
        let b_released = self.prev_buttons & !current_b;
        self.prev_buttons = current_b;
        for bit in 0..3u8 {
            let key = Self::bit_to_button(bit);
            if b_pressed  & (1 << bit) != 0 { events.push(G13Event::KeyDown(key)); }
            if b_released & (1 << bit) != 0 { events.push(G13Event::KeyUp(key)); }
        }
```

Add the helper next to `bit_to_mkey`:

```rust
    fn bit_to_button(bit: u8) -> G13Key {
        match bit {
            0 => G13Key::Btn1,
            1 => G13Key::Btn2,
            2 => G13Key::Stick,
            _ => unreachable!(),
        }
    }
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test protocol:: 2>&1 | tail -5`
Expected: PASS (all protocol tests, including the 4 new ones).
Run: `cargo test 2>&1 | tail -3` → full suite green (the `rows_cover_all_22_keys_once` monitor test still passes — `ROWS` is still the 22 G-keys; `Btn1`/`Btn2`/`Stick` are not in `ROWS`).

- [ ] **Step 5: Commit**

```bash
git add src/protocol.rs
git commit -m "feat: decode thumb buttons (Btn1/Btn2/Stick) from report byte 7

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: Config names for the thumb buttons

**Files:** Modify `src/config.rs`.

**Interfaces:**
- Consumes: `G13Key::{Btn1,Btn2,Stick}` (Task 1).

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `src/config.rs`:

```rust
    #[test]
    fn parses_thumb_button_names() {
        use crate::protocol::G13Key;
        let config = Profile::from_raw(raw(&[
            ("BTN1", "a"), ("btn2", "b"), ("Stick", "space"),
        ])).unwrap();
        assert_eq!(config.get_binding(G13Key::Btn1), Some("a"));
        assert_eq!(config.get_binding(G13Key::Btn2), Some("b"));
        assert_eq!(config.get_binding(G13Key::Stick), Some("space"));
    }

    #[test]
    fn thumb_binding_round_trips_through_toml() {
        use crate::protocol::G13Key;
        let p = Profile::from_raw(raw(&[("BTN1", "ctrl+c"), ("STICK", "enter")])).unwrap();
        let toml = p.to_toml().unwrap();
        let reloaded = Profile::from_raw(toml::from_str(&toml).unwrap()).unwrap();
        assert_eq!(reloaded.get_binding(G13Key::Btn1), Some("ctrl+c"));
        assert_eq!(reloaded.get_binding(G13Key::Stick), Some("enter"));
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test config::tests::parses_thumb_button_names config::tests::thumb_binding_round_trips_through_toml 2>&1 | tail -12`
Expected: FAIL — `parse_g13_key` returns `None` for `BTN1` (so `from_raw` errors "unknown G13 key").

- [ ] **Step 3: Implement**

In `src/config.rs`, add three arms to `parse_g13_key` (before the `_ => None` arm):

```rust
        "BTN1"  => Some(G13Key::Btn1),
        "BTN2"  => Some(G13Key::Btn2),
        "STICK" => Some(G13Key::Stick),
```

(`parse_g13_key` already uppercases its input, so `btn2` / `Stick` match. `to_toml` serializes `format!("{k:?}")` = `Btn1`/`Btn2`/`Stick`, which round-trips.)

- [ ] **Step 4: Run to verify pass**

Run: `cargo test config:: 2>&1 | tail -5`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/config.rs
git commit -m "feat: accept BTN1/BTN2/STICK binding names

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: GUI — editor section + monitor indicator

**Files:** Modify `src/monitor/mod.rs`.

**Interfaces:**
- Consumes: `G13Key::{Btn1,Btn2,Stick}` (Task 1).

- [ ] **Step 1: Add the `THUMB` const + a row-render helper**

In `src/monitor/mod.rs`, near the `ROWS` const, add:

```rust
/// The three bindable thumb inputs (byte 7): two side buttons + the joystick click.
const THUMB: [G13Key; 3] = [G13Key::Btn1, G13Key::Btn2, G13Key::Stick];
```

Add a free helper (module level, near `combo_valid`) that renders one editable binding row (so the G-key grid and the thumb section share it):

```rust
fn render_binding_row(
    ui: &mut egui::Ui,
    key: G13Key,
    edits: &mut HashMap<G13Key, String>,
    valid_keys: &HashSet<String>,
) {
    let green = egui::Color32::from_rgb(127, 224, 160);
    let red = egui::Color32::from_rgb(220, 90, 90);
    let dim = egui::Color32::from_gray(110);
    let buf = edits.entry(key).or_default();
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
    });
}
```

- [ ] **Step 2: Include THUMB in the editor reload + render**

In `render_bindings`, change the reload to include `THUMB` (chain it after the flattened rows):

```rust
            self.edits = ROWS.iter().flat_map(|row| row.iter()).chain(THUMB.iter())
                .map(|&k| (k, bound.get(&k).cloned().unwrap_or_default()))
                .collect();
```

Replace the `ScrollArea` render body (the `for row in ROWS { for &key in row { ...inline row... } }` block and the inline `green`/`red`/`dim` locals it used) with calls to the helper, plus a Thumb section:

```rust
        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            for &key in ROWS.iter().flat_map(|row| row.iter()) {
                render_binding_row(ui, key, &mut self.edits, &valid_keys);
            }
            ui.add_space(6.0);
            ui.separator();
            ui.label("Thumb buttons");
            for &key in THUMB.iter() {
                render_binding_row(ui, key, &mut self.edits, &valid_keys);
            }
        });
```

(The Save button already collects every non-empty entry in `self.edits`, which now includes the THUMB keys, so thumb bindings persist with no change to the Save code.)

- [ ] **Step 3: Add the thumb indicator to the Monitor tab**

In `render_monitor`, after the M-key indicator row (the `ui.horizontal(|ui| { ui.label("M-keys:"); ... })` block), add a thumb-button indicator:

```rust
        ui.horizontal(|ui| {
            ui.label("Thumb:");
            let hot = egui::Color32::from_rgb(127, 224, 160);
            let dim = egui::Color32::from_gray(140);
            for (key, label) in [(G13Key::Btn1, "BTN1"), (G13Key::Btn2, "BTN2"), (G13Key::Stick, "STICK")] {
                let on = snapshot.pressed.contains(&key);
                ui.colored_label(if on { hot } else { dim }, label);
            }
        });
```

- [ ] **Step 4: Build + test**

Run: `cargo build 2>&1 | tail -2` → `Finished`.
Run: `cargo test 2>&1 | tail -3` → green (no new unit tests; the editor/monitor are manual-verify). Only the pre-existing `usb.rs` warning.

- [ ] **Step 5: Commit**

```bash
git add src/monitor/mod.rs
git commit -m "feat: thumb buttons in the Bindings editor and Monitor indicator

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: Hardware smoke test + docs

**Files:** Create `milestones/finished/thumb-buttons.md`.

- [ ] **Step 1: Build release + full test**

Run: `cargo build --release 2>&1 | tail -2` → `Finished`.
Run: `cargo test 2>&1 | tail -3` → green.

- [ ] **Step 2: Hardware smoke test (manual — G13 on WinUSB)**

Run the GUI; on the **Bindings** tab (Active mode), scroll to **Thumb buttons** and set + Save on the default profile: `BTN1 = a`, `BTN2 = b`, `STICK = space`.

```bash
export PATH="$HOME/.cargo/bin:/c/Strawberry/c/bin:$PATH"
export RUST_LOG=debug
./target/release/g13-driver.exe
```
Confirm ALL of:
- The **Thumb buttons** section shows BTN1/BTN2/STICK editable rows; `a`/`b`/`space` show `ok`.
- On the **Monitor** tab, pressing each thumb button lights it in the `Thumb:` row (BTN1/BTN2/STICK), and pressing the joystick straight down lights **STICK**.
- With Notepad focused (Active mode): pressing BTN1 types `a`, BTN2 types `b`, the stick click types a space. Holding a thumb button holds the key (hold-means-hold) with no stuck key on release.
- An unbound thumb button does nothing.

- [ ] **Step 3: Milestone**

Create `milestones/finished/thumb-buttons.md`:

```markdown
# Thumb buttons (near-joystick) bindable

- **Status:** finished
- **Date:** 2026-07-05

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

## Follow-ups
- MR as a fourth bindable button (same model; deferred).
```

- [ ] **Step 4: Commit**

```bash
git add milestones/finished/thumb-buttons.md
git commit -m "docs: record thumb-buttons milestone (hardware-verified)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Self-Review

**Spec coverage:**
- `G13Key::{Btn1,Btn2,Stick}` + parser byte-7 bits 1/2/3 decode → Task 1 ✓
- `parse_g13_key` names + to_toml round-trip → Task 2 ✓
- Dispatcher/DeviceState unchanged (reuse) → confirmed (no task needed; thumb buttons are `KeyDown`/`KeyUp(G13Key)`)
- Editor "Thumb buttons" section (reload+render+save include THUMB) + Monitor indicator → Task 3 ✓
- Testing (protocol decode, config parse/round-trip; GUI manual) → Tasks 1,2,4 ✓
- Milestone → Task 4 ✓

**Deviations:** none. Note: `render_bindings` is refactored to a shared `render_binding_row` helper (used by both the G-key grid and the thumb section) — a small cleanup that avoids duplicating the row widget.

**Placeholder scan:** none — every step has concrete code/commands.

**Type consistency:** `G13Key::{Btn1,Btn2,Stick}`, `ReportParser.prev_buttons`, `bit_to_button`, `THUMB: [G13Key;3]`, `render_binding_row(ui, key, &mut edits, &valid_keys)`, `parse_g13_key` arms — consistent across tasks.
