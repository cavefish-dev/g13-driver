# Minor Improvements — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Two no-behavior-change cleanups flagged during code review.

## Global Constraints
- **GNU toolchain only.** If `cargo`/`gcc` not found, prepend PATH per CLAUDE.md. Do NOT switch to MSVC.
- Both changes are **refactors with no observable behavior change** on real inputs; existing tests must still pass. One focused commit.

---

## Task 1: Two review cleanups

**Files:** Modify `src/config.rs` (`active_name_stem`), `src/dispatcher.rs` (`handle_joystick`).

**Cleanup A — `active_name_stem` uses `strip_suffix`.** In `src/config.rs`, `active_name_stem` currently does `self.active_name().map(|n| n.trim_end_matches(".toml"))`. `trim_end_matches` strips *all* trailing `.toml` occurrences (e.g. `"a.toml.toml"` → `"a"`). Change to strip exactly one:

```rust
    pub fn active_name_stem(&self) -> Option<&str> {
        self.active_name().map(|n| n.strip_suffix(".toml").unwrap_or(n))
    }
```

(The existing `active_name_stem_strips_toml_extension` test — `"basic.toml"` → `"basic"` — still passes, since a single `.toml` strips identically.)

**Cleanup B — fold the joystick repeat-flag read into the up-front snapshot.** In `src/dispatcher.rs` `handle_joystick`, the loop currently re-acquires `self.profiles.read()` per `KeyDown` action to read `joystick_repeats(dir)`. Snapshot all four direction repeat flags in the SAME up-front read that grabs `(cfg, deadzone, ar)`, and index them in the loop — removing the second lock acquisition and the TOCTOU window.

READ the current `handle_joystick` first. Change the up-front snapshot block to also capture a `[bool; 4]` of repeat flags for `[Up, Down, Left, Right]` (via `active_profile().map(|p| [...])` , `unwrap_or([false; 4])`), then in the `KeyDown { dir, .. }` arm replace the inner `let repeats = { let set = self.profiles.read()...; ... };` with an index into the snapshot by direction. Add a small `dir → 0..3` mapping (Up=0, Down=1, Left=2, Right=3), matching `JoystickDir`. Keep behavior identical (the repeat flag now comes from the same instant as `cfg`/`deadzone`, which is strictly more consistent).

- [ ] **Step 1: Apply both cleanups** per the above. No new tests needed (behavior unchanged); the existing `active_name_stem_strips_toml_extension` and `joystick_repeat_refires_on_tick` tests cover them.

- [ ] **Step 2: Verify.** `cargo test` (all pass, incl. those two) + `cargo build` clean (no new warnings; the second `profiles.read()` in `handle_joystick` should be gone).

- [ ] **Step 3: Commit** `git commit -m "refactor: strip_suffix for name stem; fold joystick repeat read into one snapshot"`

---

## Task 2: Milestone

- [ ] Create `milestones/finished/minor-improvements.md` (Status: finished) noting the two cleanups + that they're no-behavior-change review follow-ups. `cargo test && cargo build --release` clean. Commit `docs: minor-improvements milestone`.

---

## Self-Review
- Cleanup A (strip_suffix), Cleanup B (single joystick snapshot) → Task 1. ✓
- No behavior change; existing tests cover both. Build stays green.
