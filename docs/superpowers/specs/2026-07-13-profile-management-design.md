# Profile management — design

- **Status:** approved (design)
- **Date:** 2026-07-13
- **Scope:** Turn the half-built Profiles tab into a full profile manager: a profiles *library*
  (the folder of `.toml` files) that the user browses and selects from, with a relocatable +
  openable folder, and create / duplicate / rename / delete operations. The three physical M-keys
  (M1/M2/M3) remain assignable slots fed from the library. Ship with exactly two bundled profiles:
  **basic** and **media**. Windows-only, consistent with the rest of the app.

## Motivation

`config.toml` already maps M1/M2/M3 to profile files, and the Profiles tab lists the folder
read-only with a literal *"(assigning files to slots and editing bindings are planned)"* note.
This completes that: the folder becomes a managed library, slots are assigned from the UI, and the
folder can be moved somewhere synced/backed-up. Downloading profiles from GitHub is a **later**
step; this release only manages local files and ships two starter profiles.

## Decisions (from brainstorming)

- **M-keys = assignable slots fed from the library** (not an open-ended active-profile picker). The
  folder is the library; M1/M2/M3 each hold one library profile; the active profile is the active
  slot, switched by the physical M-key or by clicking the slot (unchanged behavior).
- **Selecting = assigning to the active slot.** Clicking a profile in the library list assigns it
  to whichever slot is currently active and takes effect immediately. Per-row buttons handle
  Duplicate / Rename / Delete so those don't collide with click-to-assign.
- **Folder relocation copies the library** into the new folder (skipping name collisions), leaving
  the originals as a backup, then re-points at the new folder.
- **Profiles have a friendly `[meta].name`** stored inside the file; the filename is incidental and
  generated. This buys a true in-place Rename (filename stays stable, so manifest slot references
  never break). Legacy files without `[meta]` fall back to their filename stem — no migration.
- **Persistence:** disk is the source of truth. Every mutation writes to disk then synchronously
  reloads the shared `ProfileSet` (approach #1). Manifest edits are format-preserving (`toml_edit`).
- **Folder picker:** native dialog via the `rfd` crate (de-risked on the GNU toolchain in Task 1).
- **Ship two profiles:** basic + media; drop game.

## Architecture

```
profiles/*.toml  <--(list/create/duplicate/rename/delete/copy_into)--  src/profiles.rs   (file library, pure)
config.toml      <--(persist_slot / persist_profiles_dir / persist_start_active)-------  ProfileSet (config.rs)
shared state     <--(reload_now: load + swap under write lock, preserve active)---------  runtime.rs
Profiles tab (monitor/mod.rs) calls the above; rfd + explorer.exe are #[cfg(windows)]
```

- **`src/profiles.rs`** owns the file library: takes a folder path, no GUI/OS types, unit-tested.
- **`ProfileSet`** (in `config.rs`) owns manifest persistence (it holds `config_path`), beside the
  existing `persist_start_active`.
- **`runtime`** owns the reload + the watcher, including re-pointing the watcher on folder change.
- The **Profiles tab** is the only new UI; the **Bindings tab** is unchanged (it already edits the
  active slot's profile).

## Data model & profile file schema

A profile file gains an optional namespaced name:

```toml
[meta]
name = "Basic"

[keys]
G1 = "ctrl+c"
# … unchanged: [keys] / [joystick] / [repeat] …
```

- **Display name** = `[meta].name` if present and non-empty, else the filename stem. Legacy files
  (no `[meta]`) show their stem and keep working — **no migration**.
- `RawConfig` gets `#[serde(default)] meta: Option<RawMeta>` with `name: Option<String>`.
  `Profile` carries the resolved display name; `to_toml()` writes `[meta].name` when set.
- **Rename edits only `[meta].name`** and leaves the file in place. The manifest references slots by
  filename (`m1 = "basic.toml"`), so rename never breaks a slot assignment.
- **Filename generation** (New / Duplicate): `slug(name)` = lowercase, spaces/`_` → `-`, drop
  anything outside `[a-z0-9-]`, collapse repeated `-`, trim. Empty-after-slug (e.g. emoji-only) →
  `profile`. `unique_filename(dir, name)` suffixes `-2`, `-3`, … until free. "My Games!" →
  `my-games.toml`; a second → `my-games-2.toml`.
- **Manifest structure unchanged:** `profiles_dir` + `m1/m2/m3`. Assignment rewrites the `mN`
  filename; relocation rewrites `profiles_dir` (absolute path once relocated).

## Library module (`src/profiles.rs`) & manifest helpers

`src/profiles.rs` (folder-path in, pure, testable):

- `struct ProfileEntry { filename: String, display_name: String }`
- `fn list(dir) -> Vec<ProfileEntry>` — every `.toml`, display name resolved, sorted by display
  name (case-insensitive).
- `fn slug(name) -> String`; `fn unique_filename(dir, display_name) -> String`.
- `fn create(dir, display_name) -> Result<String>` — writes a blank profile (`[meta].name` + empty
  `[keys]`); returns the filename.
- `fn duplicate(dir, src_filename, new_display_name) -> Result<String>` — loads source, sets new
  name, writes under a fresh filename.
- `fn rename(dir, filename, new_display_name) -> Result<()>` — rewrites only `[meta].name` via
  `toml_edit` (preserves bindings/comments); filename untouched.
- `fn delete(dir, filename) -> Result<()>` — removes the file.
- `fn copy_into(src_dir, dst_dir) -> CopyReport` — copy every `.toml`, **skip** existing; returns
  `{ copied: usize, skipped: usize }`.

Manifest mutators on `ProfileSet` (format-preserving `toml_edit`, beside `persist_start_active`):

- `persist_slot(mkey, Option<&str>) -> Result<()>` — set/clear `m1|m2|m3`.
- `persist_profiles_dir(&Path) -> Result<()>` — set `profiles_dir`.

Sync helper in `runtime`:

- `reload_now(&Arc<RwLock<ProfileSet>>, config_path) -> Result<()>` — `ProfileSet::load` + swap
  under the write lock, preserving the current active M-key when its slot still resolves, else
  falling back to M1. Every UI mutation calls a disk op then `reload_now`.

## UI — rebuilt Profiles tab (`src/monitor/mod.rs`)

Top to bottom:

- **Slots:** rows M1 / M2 / M3, each a `SelectableLabel` showing the assigned profile's display
  name (`M1 — Basic`) or `M1 — (unassigned)`. Active slot highlighted. Click a slot → make active
  (`set_active`, unchanged).
- **Helper line:** "Click a slot to make it active, then click a profile below to assign it to that
  slot."
- **Folder bar:** current folder path (elided if long) + **Change folder…** (`rfd` →
  copy-into-new + re-point), **Open folder** (`explorer.exe <dir>`, Windows-gated), **New** (name
  prompt).
- **Library list:** scroll area, one row per `ProfileEntry`: display name as a clickable label →
  assign to the active slot + reload (immediate); the active slot's profile row is highlighted.
  Trailing per-row buttons: **Duplicate**, **Rename**, **Delete**.
- **Name prompt** (New / Duplicate / Rename): `egui::Modal` with one text field + OK/Cancel. New
  empty; Duplicate preloads "Copy of <name>"; Rename preloads the current name. OK disabled while
  blank.
- **Delete confirmation:** `egui::Modal` — *"Delete profile 'Media'? This removes the file."* →
  Delete / Cancel.
- **Status line:** transient success/error string at the bottom (mirrors the Bindings tab's
  `save_status`).

New `MonitorApp` fields: `name_prompt: Option<{kind, target, buffer}>`,
`pending_delete: Option<String>`, `profiles_status: Option<String>`.

## Persistence, sync, folder relocation & guardrails

- **Disk-first, then `reload_now`** for every mutation; the async watcher reloads identical content
  (idempotent).
- **Folder relocation:** `rfd` picks a dir → `copy_into(old, new)` (skip collisions) →
  `persist_profiles_dir(new)` → `reload_now` → **re-point the watcher**: after each reload the
  watcher thread compares the freshly-loaded `profiles_dir` to the dir it is watching; if changed,
  `unwatch(old)` + `watch(new)`. Status: "Copied N profiles (skipped M already present)."
- **Load invariant:** the manifest's `m1` must always resolve or `ProfileSet::load` errors. All
  guardrails protect it:
  - **Delete** (with confirmation): allowed; auto-clears any **M2/M3** slot referencing the file.
    **Refused** if the file is bound to **M1** ("Reassign M1 first") or is the **last profile** in
    the folder ("Can't delete the only profile"). If the deleted profile was active (via M2/M3),
    active falls back to M1.
  - **Assign** just overwrites the active slot's `mN`.
  - **Bad input / collisions:** blank or slug-empty names rejected with a status message;
    folder-copy never overwrites; an unreadable chosen folder reports an error and leaves the
    current folder in place.
  - **No panics:** every op returns `Result`; failures land in the status line and leave the app on
    the current state (project error policy).

## Shipped profiles, packaging & upgrade behavior

- **Bundle exactly two files** (replacing today's three):
  - `profiles/basic.toml` = today's `default.toml` content + `[meta] name = "Basic"`.
  - `profiles/media.toml` = today's `media.toml` content + `[meta] name = "Media"`.
  - **Delete** `profiles/default.toml` and `profiles/game.toml` from the repo.
- **Default manifest** (`config.toml`): `profiles_dir = "profiles"`, `m1 = "basic.toml"`,
  `m2 = "media.toml"`, `m3` omitted. Startup active = M1 (basic).
- **Packaging/CI:** `release.yml` already `cp -r profiles "$STAGE"/`; it ships whatever's in the
  folder — no workflow change.
- **Upgrade behavior (call out, not a bug):** auto-update preserves the user's `config.toml` +
  `profiles/`, so existing 0.1.x users **keep** their `default.toml`/`game.toml` (loader unchanged,
  display names fall back to stems). basic/media only appear on **fresh installs**. No force-migration
  (that's the deferred "add-missing-profiles on update" follow-up).

## Testing

- **Unit (TDD, pure logic):**
  - `profiles.rs`: `slug()` (spaces/symbols/emoji-only→`profile`/collapsing); `unique_filename()`
    suffixing; `list()` display-name resolution (`[meta].name` vs stem) + sort; `create` (blank +
    resolvable name); `duplicate` (copies bindings, new filename/name); `rename` (only `[meta].name`
    changes, bindings/comments preserved); `delete`; `copy_into` (copies + skips collisions, correct
    `{copied, skipped}`).
  - `config.rs`: `[meta].name` parse + empty/absent fallback; `to_toml()` name round-trip;
    `persist_slot` / `persist_profiles_dir` mutate only their key, preserve comments/other keys,
    reload correctly (mirrors the `persist_start_active` test).
  - **Guardrail logic** (extracted from the GUI so it's testable): "can this profile be deleted?"
    given manifest slots + folder contents (refuse M1-bound / last-profile; auto-unassign M2/M3);
    active-fallback-to-M1.
- **Manual-verify (documented exception, no unit tests — same as USB/`SendInput`/self-replace):**
  the `rfd` folder picker, `explorer.exe` open-folder, and the live watcher **re-point** after a
  folder change. A short smoke-test checklist goes in the milestone.

## Dependencies (new)

- `rfd` (native folder picker) — Windows COM file dialog; **de-risk the GNU-toolchain build in
  Task 1**, fall back to a path text field only if it won't compile under MinGW.

## Out of scope (follow-ups)

- Downloading / browsing profiles from GitHub (the next step after this).
- Force-migrating existing installs to the new bundled profiles ("add-missing-profiles on update").
- Per-slot unassign UI beyond what delete/reassign already provide; drag-and-drop assignment.
- Non-Windows folder picker / open-folder.
- `%APPDATA%` config location (still deferred from earlier specs).
