# MR Mode Toggle + LCD Filename Stem — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Repurpose MR to toggle dry-run/active in both the GUI and headless (headless gaining a dry-run mode), and show the LCD profile name without the `.toml` extension.

**Architecture:** Feature 2 is a small `ProfileSet` accessor used by the two LCD model builders. Feature 1 flips the existing `dry_run` AtomicBool from the event loop on `MKeyDown(MR)` in the GUI, and adds an equivalent `dry_run` flag + active-gating to `run_headless` (mirroring the GUI `consumer_loop`), also feeding it to the headless LCD poller as the mode source.

**Tech Stack:** Rust. No new dependencies.

## Global Constraints

- **GNU toolchain only.** Build/test with `stable-x86_64-pc-windows-gnu`; MinGW gcc at `C:\Strawberry\c\bin\gcc.exe`. If `cargo`/`gcc` not found, prepend to PATH per CLAUDE.md. Do NOT switch to the MSVC target.
- **TDD** for the pure accessor (`active_name_stem`). The MR-toggle / headless dry-run gating is event-loop wiring — **no new unit test** (reuses the established GUI dry-run pattern; verified by the hardware smoke test), same policy as the existing `consumer_loop`.
- **Error policy:** no `panic!`/`unwrap()` on the runtime path beyond the accepted `.lock().unwrap()`/`.read().unwrap()` lock idiom.
- MR must fire no keystroke (dispatcher already no-ops MR); `capture` already ignores M-keys. Headless boots **Active** (`dry_run=false`); the mode is not persisted in headless.
- One focused commit per task; imperative subject line.

---

## File Structure
- **Modify** `src/config.rs` — add `ProfileSet::active_name_stem()`.
- **Modify** `src/lcd/mod.rs` — poller uses `active_name_stem()`.
- **Modify** `src/monitor/mod.rs` — `render_lcd` uses `active_name_stem()`; `consumer_loop` MR toggle.
- **Modify** `src/runtime.rs` — `run_headless` dry-run flag + gating + MR toggle.
- **Modify** `milestones/open/pre-release-polish.md` → `milestones/ongoing/` with smoke checklist.

---

## Task 1: `active_name_stem` + LCD uses it (Feature 2)

**Files:**
- Modify: `src/config.rs` (add accessor after `active_name`, ~line 588)
- Modify: `src/lcd/mod.rs` (poller, line 220)
- Modify: `src/monitor/mod.rs` (`render_lcd`, line 1306)
- Test: `src/config.rs` (`profileset_tests` module)

**Interfaces:**
- Produces: `pub fn active_name_stem(&self) -> Option<&str>` on `ProfileSet`.

- [ ] **Step 1: Write the failing test**

Add to the `profileset_tests` module in `src/config.rs`:

```rust
#[test]
fn active_name_stem_strips_toml_extension() {
    let d = std::env::temp_dir().join("g13-cfg-stem");
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("profiles")).unwrap();
    std::fs::write(d.join("profiles/basic.toml"), "[keys]\nG1 = \"a\"\n").unwrap();
    std::fs::write(d.join("config.toml"),
        "profiles_dir = \"profiles\"\nm1 = \"basic.toml\"\n").unwrap();

    let set = ProfileSet::load(&d.join("config.toml")).unwrap();
    assert_eq!(set.active_name(), Some("basic.toml"));   // raw filename unchanged
    assert_eq!(set.active_name_stem(), Some("basic"));   // stem drops .toml
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test config::profileset_tests::active_name_stem_strips_toml_extension`
Expected: FAIL — `no method named active_name_stem`.

- [ ] **Step 3: Write minimal implementation**

In `src/config.rs`, immediately after `pub fn active_name(&self) -> Option<&str> { self.name(self.active) }`:

```rust
    /// The active profile's filename without the `.toml` extension (for the LCD).
    pub fn active_name_stem(&self) -> Option<&str> {
        self.active_name().map(|n| n.trim_end_matches(".toml"))
    }
```

Then update the two LCD model builders to use it:

- `src/lcd/mod.rs:220` — change `profile_name: set.active_name().map(str::to_string),` to
  `profile_name: set.active_name_stem().map(str::to_string),`
- `src/monitor/mod.rs:1306` (inside `render_lcd`) — change `profile_name: set.active_name().map(str::to_string),` to
  `profile_name: set.active_name_stem().map(str::to_string),`

(Do NOT touch the other `active_name()` uses in `monitor/mod.rs` — lines ~564, ~1032 — those are the Monitor/Bindings tabs, out of scope.)

- [ ] **Step 4: Run test + build**

Run: `cargo test config::profileset_tests::active_name_stem_strips_toml_extension` then `cargo build`
Expected: PASS; build clean.

- [ ] **Step 5: Commit**

```bash
git add src/config.rs src/lcd/mod.rs src/monitor/mod.rs
git commit -m "feat(lcd): show active profile filename without .toml extension"
```

---

## Task 2: MR toggles dry-run/active (GUI + headless) (Feature 1)

**Files:**
- Modify: `src/monitor/mod.rs` (`consumer_loop`, ~line 414)
- Modify: `src/runtime.rs` (`run_headless`, lines ~44-79 + imports)

**Interfaces:**
- Consumes: existing `dry_run: Arc<AtomicBool>` (GUI); a new local `dry_run` flag (headless); `crate::protocol::{G13Event, MKey}`.

**Note:** event-loop wiring — **no unit test** (per Global Constraints). Verification = clean `cargo build` + existing `cargo test` green + the Task 3 hardware smoke test. Both edits must keep the build green.

- [ ] **Step 1: GUI — flip `dry_run` on `MKeyDown(MR)`**

In `src/monitor/mod.rs` `consumer_loop`, inside the `Ok(event) =>` arm, immediately after `crate::lcd::capture(&event, &profiles, &last_action);` (currently line 416) and before `let active = !dry_run.load(Ordering::Relaxed);`, insert:

```rust
                if let G13Event::MKeyDown(crate::protocol::MKey::MR) = event {
                    dry_run.store(!dry_run.load(Ordering::Relaxed), Ordering::Relaxed);
                }
```

(`event` is `G13Event` which is `Copy`, so matching it by value here does not move it; it is still dispatched below. The subsequent `let active = ...` reads the just-updated flag, and the existing `was_active && !active` branch releases held keys on an active→dry transition. `ctx.request_repaint()` at the end of the arm refreshes the UI.)

- [ ] **Step 2: Headless — add a `dry_run` flag, gating, and the MR toggle**

In `src/runtime.rs`, add to the imports at the top (near `use std::sync::{Arc, Mutex, RwLock};`):

```rust
use std::sync::atomic::{AtomicBool, Ordering};
```

In `run_headless`, replace the LCD-mode line (currently line 48):

```rust
    let lcd_mode = Arc::new(std::sync::atomic::AtomicBool::new(false)); // headless = always Active
    crate::lcd::spawn_poller(config.clone(), lcd_mode, last_action.clone(), lcd_frame.clone());
```

with a real dry-run flag that both the poller and the dispatch loop share:

```rust
    // Headless starts Active (injecting); MR toggles this at runtime. Not persisted.
    let dry_run = Arc::new(AtomicBool::new(false));
    crate::lcd::spawn_poller(config.clone(), dry_run.clone(), last_action.clone(), lcd_frame.clone());
```

Then replace the dispatch loop (currently lines ~66-79):

```rust
    loop {
        match rx.recv_timeout(Duration::from_millis(15)) {
            Ok(event) => {
                crate::lcd::capture(&event, &config, &last_action);
                if let G13Event::MKeyDown(crate::protocol::MKey::MR) = event {
                    dry_run.store(!dry_run.load(Ordering::Relaxed), Ordering::Relaxed);
                }
                let active = !dry_run.load(Ordering::Relaxed);
                if was_active && !active {
                    dispatcher.release_held();
                }
                if active {
                    if let Err(e) = dispatcher.handle(event) {
                        log::warn!("dispatch error: {e:#}");
                    }
                }
                was_active = active;
                dispatcher.tick(Instant::now());
            }
            Err(RecvTimeoutError::Timeout) => {
                let active = !dry_run.load(Ordering::Relaxed);
                if was_active && !active {
                    dispatcher.release_held();
                }
                was_active = active;
                dispatcher.tick(Instant::now());
            }
            // The supervisor keeps tx alive, so this only fires if it died: exit safely.
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
```

Declare `was_active` just before the loop (headless starts Active):

```rust
    let mut was_active = true;
    loop {
```

- [ ] **Step 3: Build + test**

Run: `cargo build` then `cargo test`
Expected: clean build; all existing tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/monitor/mod.rs src/runtime.rs
git commit -m "feat(mkey): MR toggles dry-run/active in GUI and headless"
```

---

## Task 3: Milestone + hardware smoke test

**Files:**
- Move: `milestones/open/pre-release-polish.md` → `milestones/ongoing/pre-release-polish.md`

- [ ] **Step 1: Update the milestone and move it**

Edit `milestones/open/pre-release-polish.md`: set `Status: ongoing`, `Updated: 2026-07-17`, check the implemented task boxes, and add:

```markdown
## Hardware smoke test (manual)
- [ ] GUI: press MR on the device → injection stops (Dry-run) / starts (Active),
      and the LCD mode box + tray/UI reflect it.
- [ ] Headless (`--headless`): press MR → keystrokes stop/start injecting; the
      LCD mode box flips ACTIVE↔DRY-RUN.
- [ ] M1/M2/M3 still switch profiles in both runtimes.
- [ ] LCD profile line shows the filename without `.toml` (e.g. `basic`).
```

Then move it:

```bash
git mv milestones/open/pre-release-polish.md milestones/ongoing/pre-release-polish.md
```

- [ ] **Step 2: Full build + test**

Run: `cargo test && cargo build --release`
Expected: all tests pass; release binary builds clean.

- [ ] **Step 3: Commit**

```bash
git add milestones/
git commit -m "docs: pre-release-polish milestone to ongoing with smoke checklist"
```

---

## Self-Review

**Spec coverage:**
- Feature 2 (`active_name_stem` + both LCD builders) → Task 1. ✓
- Feature 1 GUI MR toggle → Task 2 Step 1. ✓
- Feature 1 headless dry-run mode + MR toggle + LCD mode source → Task 2 Step 2. ✓
- M1/M2/M3 unchanged; MR fires no keystroke (dispatcher no-op); capture ignores M-keys → no change needed, noted. ✓
- Headless boots Active, not persisted → Task 2 Step 2 (`AtomicBool::new(false)`, no persist). ✓
- Smoke test → Task 3. ✓

**Placeholder scan:** none — all steps have complete code.

**Type consistency:** `active_name_stem() -> Option<&str>` used identically in both LCD builders; `dry_run: Arc<AtomicBool>` with `Ordering::Relaxed`; `G13Event`/`MKey::MR` pattern identical in both runtimes; `spawn_poller(config, dry_run, last_action, lcd_frame)` signature matches the existing LCD poller.
