# Centered Clock + MR Dry-run LED — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Center the line-1 clock, and light the MR LED during dry-run (gated by `mkey_indicator`).

**Tech Stack:** Rust. No new dependencies.

## Global Constraints
- **GNU toolchain only.** Build/test with `stable-x86_64-pc-windows-gnu`; MinGW gcc at `C:\Strawberry\c\bin\gcc.exe`. If `cargo`/`gcc` not found, prepend to PATH per CLAUDE.md. Do NOT switch to the MSVC target.
- **TDD** for `resolve` (pure). GUI/USB unchanged here; `render` clock change is verified by a pixel test + manual.
- **Error policy:** no `panic!`/`unwrap()` on the runtime path beyond the accepted lock idiom.
- MR LED = M-key bitmask bit `8`. MR-on-dry-run is **gated by `cfg.mkey_indicator`** (off → all M-key LEDs including MR stay dark).
- One focused commit per task; imperative subject.

## File Structure
- **Modify** `src/lcd/mod.rs` — center the clock in `render`.
- **Modify** `src/led/mod.rs` — `resolve` + `spawn_poller` gain `dry_run`.
- **Modify** `src/config.rs` — `desired_led_state` gains `dry_run`.
- **Modify** `src/runtime.rs`, `src/monitor/mod.rs` — pass the dry-run flag to `led::spawn_poller`.
- **Modify** milestone.

---

## Task 1: Center the line-1 clock

**Files:** Modify `src/lcd/mod.rs`; Test `src/lcd/mod.rs`.

- [ ] **Step 1: Write a failing test** (in `src/lcd/mod.rs` tests):

```rust
#[test]
fn render_clock_is_centered() {
    let mut cfg = LcdConfig::default();
    cfg.line1_clock = true;
    cfg.line1_mode = ModeDisplay::Off; // isolate the clock
    let mut m = model(None); // existing test helper
    m.clock = Some("12:34".into());
    let fb = render(&m, &cfg);
    // "12:34" is 30px wide, centered at x = (160-30)/2 = 65..95 → lit pixels in the center band.
    assert!((65..95).any(|x| (0..8).any(|y| fb.get(x, y))));
    // ...and nothing in the old far-right clock spot (x ~130..160 on the title row).
    assert!(!(130..160).any(|x| (0..8).any(|y| fb.get(x, y))));
}
```

- [ ] **Step 2: Run → fail.** `cargo test lcd::render_clock_is_centered` (clock currently draws right-clustered, so the far-right assertion fails).

- [ ] **Step 3: Implement.** In `render` (`src/lcd/mod.rs`), remove the clock from the right-cluster block (the trailing `if cfg.line1_clock { if let Some(clk) = &model.clock { x -= text_width(clk, 1); fb.draw_text(x, 0, clk, 1); } }`), so the right cluster is mode-only (also remove the now-pointless `x -= 3; // gap before clock` if it's only there for the clock — leave the mode box drawing intact). Then, after the right cluster (or anywhere on line 1 after the left text), add:

```rust
    // Clock: centered on line 1.
    if cfg.line1_clock {
        if let Some(clk) = &model.clock {
            let cx = (LCD_W as i32 - text_width(clk, 1)) / 2;
            fb.draw_text(cx, 0, clk, 1);
        }
    }
```

- [ ] **Step 4: Run → pass + full `cargo test`.**
- [ ] **Step 5: Commit** `git commit -m "feat(lcd): center the line-1 clock"`

---

## Task 2: MR LED on during dry-run (thread `dry_run` through the LED path)

**Files:** Modify `src/led/mod.rs` (`resolve`, `spawn_poller`), `src/config.rs` (`desired_led_state`), `src/runtime.rs`, `src/monitor/mod.rs`. Test `src/led/mod.rs`, `src/config.rs`.

This one change touches the signature of `resolve`/`desired_led_state`/`spawn_poller` and their call sites together, so the build stays green.

**Interfaces:**
- `resolve(active: MKey, dry_run: bool, cfg: &BacklightConfig) -> LedState`
- `ProfileSet::desired_led_state(&self, dry_run: bool) -> LedState`
- `led::spawn_poller(config, dry_run: Arc<AtomicBool>, desired)`

- [ ] **Step 1: Write/adjust failing tests.**

In `src/led/mod.rs` tests, update the existing `resolve_*` tests to the new signature (add `false`), and add:

```rust
#[test]
fn resolve_mr_lights_in_dry_run_when_indicator_on() {
    let cfg = cfg(); // existing helper: mkey_indicator = true
    assert_eq!(resolve(MKey::M1, true, &cfg).mkeys, 1 | 8); // M1 + MR
    assert_eq!(resolve(MKey::M1, false, &cfg).mkeys, 1);    // active only
}

#[test]
fn resolve_dry_run_mr_gated_by_indicator() {
    let mut c = cfg();
    c.mkey_indicator = false;
    assert_eq!(resolve(MKey::M1, true, &c).mkeys, 0); // indicator off → nothing, even in dry-run
}
```

In `src/config.rs`, update the existing `desired_led_state()` test call to `desired_led_state(false)` (asserts `mkeys: 1` at default M1/active).

- [ ] **Step 2: Run → fail.** `cargo test led:: config::` (signature mismatch / MR bit).

- [ ] **Step 3: Implement.**

`src/led/mod.rs` `resolve` — new signature + MR bit:

```rust
pub fn resolve(active: MKey, dry_run: bool, cfg: &BacklightConfig) -> LedState {
    // ... rgb unchanged ...
    let mkeys = if cfg.mkey_indicator {
        let active_bit = match active {
            MKey::M1 => 1, MKey::M2 => 2, MKey::M3 => 4, MKey::MR => 0,
        };
        active_bit | if dry_run { 8 } else { 0 }
    } else {
        0
    };
    LedState { rgb, mkeys }
}
```

`src/config.rs` `desired_led_state`:

```rust
    pub fn desired_led_state(&self, dry_run: bool) -> crate::led::LedState {
        crate::led::resolve(self.active, dry_run, &self.backlight)
    }
```

`src/led/mod.rs` `spawn_poller`:

```rust
pub fn spawn_poller(config: Arc<RwLock<ProfileSet>>, dry_run: Arc<AtomicBool>, desired: Arc<Mutex<LedState>>) {
    thread::spawn(move || loop {
        let state = config.read().unwrap().desired_led_state(dry_run.load(Ordering::Relaxed));
        *desired.lock().unwrap() = state;
        thread::sleep(Duration::from_millis(150));
    });
}
```

(Add `use std::sync::atomic::{AtomicBool, Ordering};` to `src/led/mod.rs` if not present.)

**Call sites:**
- `src/runtime.rs` `run_headless`: MOVE `let dry_run = Arc::new(AtomicBool::new(false));` (currently ~line 50) UP to before the LED `desired` init (~line 45). Change the LED `desired` init to `config.read().unwrap().desired_led_state(dry_run.load(Ordering::Relaxed))` and the LED `spawn_poller(config.clone(), desired.clone())` to `spawn_poller(config.clone(), dry_run.clone(), desired.clone())`. Leave the later `lcd::spawn_poller(config.clone(), dry_run.clone(), ...)` as-is (dry_run now exists earlier). Ensure `Ordering` is imported (it is, from the MR-toggle feature).
- `src/monitor/mod.rs` `start_consumer`: change `crate::led::spawn_poller(self.profiles.clone(), desired.clone())` (~line 367) to `crate::led::spawn_poller(self.profiles.clone(), self.dry_run.clone(), desired.clone())`. Also update the LED `desired` cell init in `start_consumer` (`...desired_led_state()`) to `desired_led_state(self.dry_run.load(Ordering::Relaxed))`.
- Fix any other `desired_led_state()` call site the compiler flags (pass the appropriate flag, or `false` if none applies).

- [ ] **Step 4: Run → pass + `cargo build` + full `cargo test`.**
- [ ] **Step 5: Commit** `git commit -m "feat(led): light MR LED in dry-run (gated by mkey_indicator)"`

---

## Task 3: Milestone + smoke

**Files:** Move `milestones/open/clock-center-mr-dryrun.md` → `milestones/ongoing/`.

- [ ] **Step 1:** Set `Status: ongoing`, check the boxes, add a smoke checklist (clock centered on LCD; MR LED lights in dry-run / dark in active; MR stays dark in dry-run when the M-key indicator is off). `git mv` to `ongoing/`.
- [ ] **Step 2:** `cargo test && cargo build --release` — all pass, clean.
- [ ] **Step 3: Commit** `git commit -m "docs: clock-center + MR-dry-run milestone to ongoing"`

---

## Self-Review
- Clock centered → Task 1. ✓
- MR LED dry-run (resolve+desired_led_state+poller+both runtimes, gated by mkey_indicator) → Task 2. ✓
- Build-green: Task 2 changes the shared signatures + all call sites together. ✓
- Types: `resolve(active, dry_run, cfg)`, `desired_led_state(dry_run)`, `spawn_poller(config, dry_run, desired)` consistent across led/config/runtime/monitor.
