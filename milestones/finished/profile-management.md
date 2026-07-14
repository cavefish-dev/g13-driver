# Profile management

- **Status:** finished — GUI smoke-tested end-to-end (2026-07-14)
- **Date:** 2026-07-14

## Outcome
The Profiles tab is now a full manager over a folder-based profile *library*. Spec:
`docs/superpowers/specs/2026-07-13-profile-management-design.md` (+ the 2026-07-14 empty-slots
revision at its top); plans: `docs/superpowers/plans/2026-07-13-profile-management.md` and
`docs/superpowers/plans/2026-07-14-empty-slots.md`.

- **Library** (`src/profiles.rs`, pure/tested): list, create, duplicate, rename (edits only the
  friendly `[meta].name`, filename stays stable), delete, `copy_into` (skips collisions), plus
  `slug`/`unique_filename` and `deletion_plan`.
- **Schema:** each profile carries a friendly `[meta].name`; display name falls back to the filename
  stem, so legacy files need no migration.
- **Slots:** M1/M2/M3 are assignable slots fed from the library. Clicking a library profile assigns
  it to the active slot; the physical M-keys still switch. **Empty slots are valid** — any slot is
  selectable, and an empty active slot makes the driver idle (injects nothing). All three slots are
  symmetric `Option<Profile>`; `active_profile()` is `Option`; the "m1 must resolve" invariant was
  removed.
- **UI controls** (`src/monitor/mod.rs`): New profile, Duplicate, Rename, Delete (confirmed;
  unconditional — auto-unassigns every referencing slot), **Unassign profile**, Change folder…
  (native `rfd` picker, copies the library into the new folder skipping collisions, re-points the
  file watcher), Open folder (`explorer.exe`). Every mutation writes to disk then
  `runtime::reload_now` (disk is source of truth). All fallible ops report to a status line — no
  panics.
- **Manifest mutators** (`ProfileSet`): `persist_slot`, `persist_profiles_dir` (format-preserving
  `toml_edit`, beside `persist_start_active`).
- **Shipped profiles:** exactly two — `basic.toml` (name "Basic") + `media.toml` (name "Media");
  `default.toml`/`game.toml` removed; default manifest `m1=basic`, `m2=media`, M3 unassigned.

Built via subagent-driven-development across two plans (Tasks 1–8 then the empty-slots revision
9–11), 136 unit tests. New dep: `rfd` 0.15 (native folder picker — confirmed building under
MinGW/GNU). Auto-update preserves the user's `config.toml`/`profiles/`, so existing installs keep
their own profiles (the basic/media rename only affects fresh installs — no force-migration).

## Reviews & the one bug caught
- Per-task reviews throughout; two final whole-branch reviews (opus): the Tasks 1–8 review returned
  MERGE (no Critical), and the empty-slots core (Task 9) got a deep adversarial review confirming an
  empty active slot can never inject or fall back to another slot.
- **Critical caught + fixed mid-flow:** a self-deadlock in `assign_to_active` — a `RwLock` read
  guard held across `reload_now`'s write lock (non-reentrant `std::sync::RwLock`). Fixed by splitting
  the statement so the guard drops first (commit 6cb4ce3); the same pattern is used in
  `change_folder`/`unassign_active`. The final review audited all mutation paths as deadlock-free.

## Smoke test — PASSED 2026-07-14 (documented GUI/OS manual-verify exception)
Verified live in the GUI: assign to a slot (the previously-deadlocking path — no hang); New
profile / Duplicate / Rename / Delete (M1-bound now deletable, cascades unassign); selecting an
empty slot (Bindings shows the empty notice, keys inject nothing); Unassign clears the active slot;
Change folder (native picker + copy) and Open folder; all-slots-empty → driver idles.

## Follow-ups (deferred)
- Download / browse profiles from GitHub (the intended next step).
- `copy_into`: cap on a huge source dir; canonicalize the `src==dst` comparison.
- `usb.rs:25` pre-existing `unused_mut` warning (unrelated; the only remaining build warning).
- Non-Windows folder picker / open-folder; `%APPDATA%` config location.
