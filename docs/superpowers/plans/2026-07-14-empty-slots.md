# Empty Slots Implementation Plan (profile-management revision)

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Make an empty M-slot (and an entirely empty slot set) a valid state — selectable, injecting nothing — plus an Unassign control and unconditional delete.

**Architecture:** `ProfileSet`'s `m1` becomes `Option<Profile>` (symmetric with `m2`/`m3`); `active_profile()` returns `Option<&Profile>`; `set_active` always succeeds for M1/M2/M3; `load` no longer requires `m1`; consumers (dispatcher, runtime, both GUI tabs) handle `None` by doing nothing / showing an empty state; `deletion_plan` collapses to "unassign all referencing slots, never refuse"; the Profiles tab gains Unassign and drops the delete refusal.

**Tech Stack:** Rust (GNU toolchain), eframe/egui 0.31.

## Global Constraints

- GNU toolchain only; if `cargo`/`gcc` missing: `export PATH="$HOME/.cargo/bin:/c/Strawberry/c/bin:$PATH"`. Run `cargo test` (never `--lib`).
- No `panic!`/`unwrap()`/`expect()` in runtime/UI paths (lock `.unwrap()` for poisoning is the accepted idiom).
- An empty active slot MUST result in the driver injecting nothing — never a panic, never a fallback to a different slot.
- Legacy bare-`[keys]` config still loads as a single M1 profile.
- Supersedes the original spec's load-invariant + delete-guardrail. Branch `feat/profile-management`.

---

### Task 9: `ProfileSet` empty-slot core + consumer ripple

Make all three slots optional and `active_profile()` an `Option`, relax `load`, make `set_active` always succeed for M1/M2/M3, and update EVERY consumer so the crate compiles. This is one atomic compile unit (the signature change ripples across files).

**Files:**
- Modify: `src/config.rs` — `ProfileSet` struct (`m1: Option<Profile>`), `load`, `active_profile`, `active_profile_mut`, `set_active`, `active_path`, `save_active_bindings`, `name`.
- Modify: `src/dispatcher.rs:50-150` — `handle_key_down`, `handle_joystick`, `handle_mkey`.
- Modify: `src/runtime.rs:40` — joystick-mode warning.
- Modify: `src/monitor/mod.rs:524-529, 555-557, ~908` — bottom panel, `render_monitor`, `render_bindings`.

**Interfaces:**
- Produces: `ProfileSet::active_profile(&self) -> Option<&Profile>`; `set_active(&mut self, MKey) -> bool` returns `true` for M1/M2/M3 (empty or not), `false` for MR; `save_active_bindings(...) -> Result<()>` returns an `Err` (or a distinct no-op) when the active slot is empty.

- [ ] **Step 1: Write failing tests** (in `src/config.rs` `mod profileset_tests`)

```rust
#[test]
fn load_with_no_m1_is_ok_and_active_is_none() {
    let d = tmp("no-m1");
    write(&d, "config.toml", "profiles_dir = \"profiles\"\n"); // no m1/m2/m3
    let set = ProfileSet::load(&d.join("config.toml")).unwrap();
    assert!(set.active_profile().is_none());
    assert_eq!(set.active(), MKey::M1);
}

#[test]
fn missing_m1_file_resolves_to_empty_not_error() {
    let d = tmp("m1-missing-file");
    write(&d, "config.toml", "profiles_dir = \"profiles\"\nm1 = \"nope.toml\"\n");
    let set = ProfileSet::load(&d.join("config.toml")).unwrap(); // was an error before
    assert!(set.active_profile().is_none());
}

#[test]
fn set_active_allows_empty_slots() {
    let d = tmp("empty-active");
    write(&d.join("profiles"), "basic.toml", "[keys]\nG1 = \"a\"\n");
    write(&d, "config.toml", "profiles_dir = \"profiles\"\nm1 = \"basic.toml\"\n");
    let mut set = ProfileSet::load(&d.join("config.toml")).unwrap();
    assert!(set.set_active(MKey::M2)); // empty, but selectable now
    assert_eq!(set.active(), MKey::M2);
    assert!(set.active_profile().is_none()); // empty active -> None
    assert!(!set.set_active(MKey::MR)); // MR still no-op
}

#[test]
fn legacy_bare_keys_still_single_m1_profile() {
    let d = tmp("legacy-empty-rev");
    write(&d, "config.toml", "[keys]\nG1 = \"ctrl+c\"\n");
    let set = ProfileSet::load(&d.join("config.toml")).unwrap();
    assert_eq!(set.active_profile().unwrap().get_binding(crate::protocol::G13Key::G1), Some("ctrl+c"));
}
```

Also FIX the existing `loads_manifest_and_switches` test: it currently asserts `!set.set_active(MKey::M3)` (empty M3 refused) — change that line to `assert!(set.set_active(MKey::M3));` and, since M3 is empty, follow with `assert!(set.active_profile().is_none());` then switch back with `set.set_active(MKey::M2);`. And any other existing test asserting `active_profile()` as a bare `&Profile` must be updated to `.unwrap()` where the slot is populated.

- [ ] **Step 2: Run tests to confirm they fail / the crate no longer compiles as-is**

Run: `cargo test 2>&1 | head -30` — expect compile errors on the signature change (that's the ripple to fix in Step 3) and the new assertions failing.

- [ ] **Step 3: Implement**

In `src/config.rs`:

Change the struct field (and update the doc comment that says "always points at a populated slot"):

```rust
    m1: Option<Profile>,
```

`load` — manifest mode: load `m1` like the others (None on missing name OR load failure, with a warning), and drop the `if let Some(m1_name)` requirement. Restructure so a manifest with NO `m1` still loads. Concretely, in manifest mode use the same `load_opt` closure for all three slots:

```rust
        // Manifest mode when ANY of profiles_dir/m1/m2/m3 is present.
        let is_manifest = raw.profiles_dir.is_some() || raw.m1.is_some()
            || raw.m2.is_some() || raw.m3.is_some();
        if is_manifest {
            let dir = base.join(raw.profiles_dir.as_deref().unwrap_or("profiles"));
            let load_opt = |name: &Option<String>| -> (Option<Profile>, Option<String>) {
                match name {
                    Some(n) => match Profile::load(&dir.join(n)) {
                        Ok(p) => (Some(p), Some(n.clone())),
                        Err(e) => { log::warn!("slot profile {n} not loaded: {e:#}"); (None, Some(n.clone())) }
                    },
                    None => (None, None),
                }
            };
            let (m1, m1_name) = load_opt(&raw.m1);
            let (m2, m2_name) = load_opt(&raw.m2);
            let (m3, m3_name) = load_opt(&raw.m3);
            Ok(Self { profiles_dir: dir, m1, m2, m3, m1_name, m2_name, m3_name,
                      active: MKey::M1, autorepeat, config_path: config_path.to_path_buf(), start_active })
        } else {
            // Legacy: the file itself is a single M1 profile.
            let m1 = Profile::load(&config_path.to_path_buf())?;
            let name = config_path.file_name().and_then(|s| s.to_str()).map(String::from);
            Ok(Self { profiles_dir: base.to_path_buf(), m1: Some(m1), m2: None, m3: None,
                      m1_name: name, m2_name: None, m3_name: None, active: MKey::M1,
                      autorepeat, config_path: config_path.to_path_buf(), start_active })
        }
```

Note: keep `m1_name` set even when the file failed to load, so the UI can show the (broken) assignment; `active_profile()` returns `None` because the `Profile` is `None`.

`active_profile` → Option, no fallback:

```rust
    pub fn active_profile(&self) -> Option<&Profile> {
        match self.active {
            MKey::M2 => self.m2.as_ref(),
            MKey::M3 => self.m3.as_ref(),
            _ => self.m1.as_ref(),
        }
    }
```

`set_active` — always succeed for M1/M2/M3:

```rust
    pub fn set_active(&mut self, k: MKey) -> bool {
        match k {
            MKey::MR => false,
            _ => { self.active = k; true }
        }
    }
```

`active_profile_mut` → `Option<&mut Profile>`:

```rust
    fn active_profile_mut(&mut self) -> Option<&mut Profile> {
        match self.active {
            MKey::M2 => self.m2.as_mut(),
            MKey::M3 => self.m3.as_mut(),
            _ => self.m1.as_mut(),
        }
    }
```

`save_active_bindings` — bail cleanly when the active slot is empty (no file to write):

```rust
    pub fn save_active_bindings(
        &mut self,
        bindings: HashMap<G13Key, String>,
        repeat: HashMap<G13Key, bool>,
    ) -> Result<()> {
        if self.active_name().is_none() || self.active_profile().is_none() {
            anyhow::bail!("no profile in the active slot");
        }
        let path = self.active_path();
        let profile = self.active_profile_mut().expect("checked above");
        profile.set_bindings(bindings);
        profile.set_repeat(repeat);
        let toml = profile.to_toml()?;
        std::fs::write(&path, toml).with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }
```

(The `.expect("checked above")` is on a value just proven `Some` two lines up — acceptable, but if you prefer, restructure with a single `let Some(profile) = self.active_profile_mut() else { bail!(...) };` before computing `path`. Either is fine; no `unwrap` on external input.)

In `src/dispatcher.rs`:

`handle_key_down` (line ~51-55) — guard the Option:

```rust
        let (binding, repeat, ar) = {
            let set = self.profiles.read().unwrap();
            match set.active_profile() {
                Some(p) => (p.get_binding(key).map(str::to_owned), p.repeats(key), set.autorepeat()),
                None => (None, false, set.autorepeat()),
            }
        };
```

`handle_joystick` (line ~126-131):

```rust
        let cfg = {
            let set = self.profiles.read().unwrap();
            set.active_profile().and_then(|p| p.joystick())
                .filter(|j| j.mode == JoystickMode::Wasd)
                .cloned()
        };
```

`handle_mkey` (line ~144-149) — `set_active` now always true for M1/M2/M3, so log the (possibly empty) target:

```rust
        let mut set = self.profiles.write().unwrap();
        if set.set_active(m) {
            log::info!("profile -> {}", set.name(m).unwrap_or("(none)"));
        }
```

In `src/runtime.rs` (line ~40):

```rust
    if let Some(j) = config.read().unwrap().active_profile().and_then(|p| p.joystick()) {
        if j.mode == JoystickMode::Mouse {
            log::warn!("joystick mouse mode is configured but not yet implemented; stick will be inert");
        }
    }
```

(If a `runtime.rs` test at line ~106 calls `active_profile().get_binding(...)`, update it to `active_profile().unwrap().get_binding(...)`.)

In `src/monitor/mod.rs`:

Bottom panel (line ~524-529):

```rust
            let set = self.profiles.read().unwrap();
            let joy = set.active_profile().and_then(|c| c.joystick())
                .map(|j| format!("joystick: {:?}, deadzone {}", j.mode, j.deadzone))
                .unwrap_or_else(|| "joystick: disabled".to_string());
            ui.label(format!("config.toml · {joy}"));
```

`render_monitor` (line ~556-557) — bind an `Option`, and where the code calls `cfg.get_binding(key)`, change to `cfg.and_then(|c| c.get_binding(key))` (empty active slot → every cell shows unmapped):

```rust
        let set = self.profiles.read().unwrap();
        let cfg = set.active_profile();
        // ... every `cfg.get_binding(key)` becomes `cfg.and_then(|c| c.get_binding(key))`
```

`render_bindings` (line ~908 area) — when the active slot is empty, show a notice and skip the editor. Right after acquiring the profile:

```rust
        let set = self.profiles.read().unwrap();
        let Some(profile) = set.active_profile() else {
            drop(set);
            ui.heading("Bindings");
            ui.label("No profile in the active slot — assign one on the Profiles tab.");
            return;
        };
```

Wire this so the existing buffer-loading logic uses `profile` (adjust the surrounding code as needed to keep it compiling — the `edits_for`/buffer reload still keys off `active_name`, which is `None` for an empty slot, so guard the reload accordingly).

- [ ] **Step 4: Run tests to confirm pass**

Run: `cargo test` — expect all green (the 4 new tests + the amended existing ones). Fix any remaining call sites the compiler flags until the crate builds and all tests pass.

- [ ] **Step 5: Build the GUI target**

Run: `cargo build` — must compile (only the pre-existing `usb.rs` warning is allowed).

- [ ] **Step 6: Commit**

```bash
git add src/config.rs src/dispatcher.rs src/runtime.rs src/monitor/mod.rs
git commit -m "feat: empty M-slots are valid — active_profile Option, driver idles when empty"
```

---

### Task 10: `deletion_plan` — unconditional, unassign all referencing slots

**Files:**
- Modify: `src/profiles.rs` — `deletion_plan`
- Test: `src/profiles.rs` (`mod tests`)

**Interfaces:**
- Produces: `deletion_plan(filename, slots: [Option<&str>; 3], total: usize) -> DeletionPlan` (NO longer a `Result`) with `unassign: Vec<MKey>` listing every slot (M1, M2, M3 in order) that referenced the file. `total` param is dropped.

- [ ] **Step 1: Replace the tests** for `deletion_plan` in `mod tests` (the old refusal tests no longer apply):

```rust
    #[test]
    fn deletion_unassigns_every_referencing_slot() {
        let plan = deletion_plan("media.toml",
            [Some("basic.toml"), Some("media.toml"), Some("media.toml")]);
        assert_eq!(plan.unassign, vec![MKey::M2, MKey::M3]);
    }

    #[test]
    fn deletion_unassigns_m1_when_bound_there() {
        let plan = deletion_plan("basic.toml",
            [Some("basic.toml"), None, None]);
        assert_eq!(plan.unassign, vec![MKey::M1]);
    }

    #[test]
    fn deletion_of_unreferenced_profile_unassigns_nothing() {
        let plan = deletion_plan("extra.toml",
            [Some("basic.toml"), Some("media.toml"), None]);
        assert!(plan.unassign.is_empty());
    }
```

Delete the old `deletion_refused_when_bound_to_m1`, `deletion_refused_when_last_profile`, `deletion_unassigns_m2_and_m3`, and `deletion_of_unassigned_profile_is_clean` tests.

- [ ] **Step 2: Run to confirm failure**

Run: `cargo test deletion_` — expect FAIL (signature mismatch / old tests gone).

- [ ] **Step 3: Implement**

```rust
/// Which M-slots must be cleared when `filename` is deleted (all that reference it).
/// Deletion is always allowed — an empty slot set is a valid state.
pub fn deletion_plan(filename: &str, slots: [Option<&str>; 3]) -> DeletionPlan {
    let mkeys = [MKey::M1, MKey::M2, MKey::M3];
    let unassign = slots.iter().zip(mkeys)
        .filter(|(s, _)| **s == Some(filename))
        .map(|(_, m)| m)
        .collect();
    DeletionPlan { unassign }
}
```

- [ ] **Step 4: Run to confirm pass**

Run: `cargo test` — all green.

- [ ] **Step 5: Commit**

```bash
git add src/profiles.rs
git commit -m "feat: deletion_plan unconditional — unassign all referencing slots"
```

---

### Task 11: Profiles tab — Unassign button + unconditional delete

Wire the two new UX behaviors. GUI — manual-verify (no unit tests).

**Files:**
- Modify: `src/monitor/mod.rs` — `render_profiles`, `try_begin_delete`, `render_delete_confirm`, add `unassign_active`.

**Interfaces:**
- Consumes: `crate::profiles::deletion_plan(filename, [Option<&str>;3]) -> DeletionPlan` (Task 10), `ProfileSet::persist_slot`, `runtime::reload_now`.

- [ ] **Step 1: Add an Unassign button**

In `render_profiles`, in the folder/action button row (next to New), add:

```rust
            if ui.button("Unassign").clicked() {
                self.unassign_active();
            }
```

Add the method to `impl MonitorApp`:

```rust
    fn unassign_active(&mut self) {
        let active = self.profiles.read().unwrap().active();
        let cleared = self.profiles.read().unwrap().persist_slot(active, None);
        let res = cleared.and_then(|_| crate::runtime::reload_now(&self.profiles, &self.config_path));
        self.profiles_status = Some(match res {
            Ok(()) => format!("Cleared {active:?}."),
            Err(e) => format!("Unassign failed: {e}"),
        });
    }
```

(Note the two-statement split — read guard dropped before `reload_now`, per the deadlock lesson.)

- [ ] **Step 2: Make delete unconditional**

`try_begin_delete` currently calls `deletion_plan(...).map_err(...)` to refuse; with Task 10 it always returns a plan. Simplify so it always opens the confirm dialog:

```rust
    fn try_begin_delete(&mut self, filename: &str) {
        self.pending_delete = Some(filename.to_string());
    }
```

Update its call site in the library-list row (it may pass `dir`/`entries` args — drop them; the deferred-action `Action::BeginDelete(filename)` path just stores the filename).

In `render_delete_confirm`, replace the plan-with-`map_err` block so it uses the non-`Result` `deletion_plan` and cascades all returned slots:

```rust
        if confirm {
            let res: anyhow::Result<()> = (|| {
                let slots_owned = {
                    let set = self.profiles.read().unwrap();
                    [set.name(MKey::M1).map(String::from),
                     set.name(MKey::M2).map(String::from),
                     set.name(MKey::M3).map(String::from)]
                };
                let slots = [slots_owned[0].as_deref(), slots_owned[1].as_deref(), slots_owned[2].as_deref()];
                let plan = crate::profiles::deletion_plan(&filename, slots);
                {
                    let set = self.profiles.read().unwrap();
                    for m in &plan.unassign { set.persist_slot(*m, None)?; }
                }
                crate::profiles::delete(dir, &filename)?;
                crate::runtime::reload_now(&self.profiles, &self.config_path)
            })();
            self.profiles_status = Some(match res {
                Ok(()) => "Deleted.".to_string(),
                Err(e) => format!("Delete failed: {e}"),
            });
            self.pending_delete = None;
        } else if cancel {
            self.pending_delete = None;
        }
```

(Keep `dir` available in `render_delete_confirm` as it is today; the M1-bound profile is now deletable — the confirm dialog wording is unchanged.)

- [ ] **Step 3: Build + manual smoke**

Run: `cargo build` then `cargo test` (should equal the Task 10 count — no new tests here).

Manual (controller/user): activate an empty slot (it highlights, Bindings shows the empty notice, keys inject nothing); assign to it; Unassign clears the active slot; delete the M1-bound profile — it's removed and M1 goes unassigned (no refusal); with all slots empty the driver injects nothing.

- [ ] **Step 4: Commit**

```bash
git add src/monitor/mod.rs
git commit -m "feat: Profiles tab — Unassign button + unconditional delete"
```

---

## Notes for the executor

- Run `cargo test` (never `--lib`). Task 9 is the atomic core (must compile across 4 files); Tasks 10–11 build on it. Task 11 is GUI manual-verify.
- After Task 11: final delta review, then continue the manual GUI smoke, then `superpowers:finishing-a-development-branch`.
