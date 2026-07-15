# Profile catalog (download / revert from GitHub) — design

- **Status:** approved (design)
- **Date:** 2026-07-15
- **Scope:** Let users browse a curated catalog of profiles hosted in the project's GitHub repo,
  download them into their local library, and **revert** an edited GitHub-sourced profile back to
  its upstream version. Pull + revert only — **not** continuous two-way sync or upstream-change
  detection (deferred). Builds directly on the profile-provenance metadata (`source`/`modified`) and
  reuses the auto-update HTTP pattern (`ureq` + `serde_json`).

## Motivation

The provenance feature added `source`/`modified` as groundwork "for downloading." This delivers the
download: a `catalog/` directory in `cavefish-dev/g13-driver` holds curated `.toml` profiles;
contributors add a file and it becomes downloadable. It also completes the deferred provenance
items — the upstream-identity field (`[meta].origin`) and the **Revert** action.

## Decisions (from brainstorming)

- **Discovery via a CI-generated index, not the Contents API.** A GitHub Action scans `catalog/*.toml`
  and writes `catalog/index.json` (`[{filename, name}]`). The app fetches that one static file over
  `raw.githubusercontent.com` (no API rate limit, CDN-cached) and gets display names in a single
  request. Contributing stays friction-free — add a `.toml`, CI regenerates the index.
- **Catalog files are plain** (`[meta].name` + bindings, no provenance fields). The app **stamps**
  `source = github`, `origin = <catalog filename>`, `modified = false` on download.
- **`origin` = the catalog filename** (not a full URL); the upstream URL is derived from a hardcoded
  base, so profiles stay portable if the repo/branch layout changes.
- **Revert restores the whole upstream profile** (bindings *and* name), re-stamping
  `source`/`origin`/`modified = false` — a local rename is discarded (the literal "make it identical
  to the GitHub version").
- **Dedup by `origin`** — the Catalog tab marks an entry "Downloaded" when a local profile already
  carries that `origin`.
- **Integrity = "it parses."** A downloaded profile is data (key mappings over TLS-verified HTTPS
  from our own repo), not an executable, and there is no trusted per-file checksum — so the guard is
  that it must parse as a valid `Profile`. (Contrast auto-update, which SHA-256s the release binary.)
- **JSON index** (not CSV) — robust escaping for display names, reuses `serde_json` (no new dep),
  extensible to `description`/`category` later.
- **UI:** a dedicated **Catalog** tab (browse + download); **Revert** on the Bindings tab beside the
  provenance note.
- **Platform-agnostic:** `src/catalog.rs` is not OS-gated (downloading a profile is `ureq` + a file
  write + TOML) — it works on every future arch/OS.

## Architecture & data flow

```
catalog/index.json  --fetch_index-->  Vec<CatalogEntry{filename,name}>  --mark_downloaded(local origins)-->  Catalog tab rows
catalog/<file>.toml --fetch_profile--> Profile (parse-validated) --stamp_download--> local library (+reload)
                                                                          \--(revert) overwrite local file wholesale
CI: push catalog/**.toml --> scan [meta].name --> write catalog/index.json --> commit back
```

- **`src/catalog.rs`** (platform-agnostic): discovery, download, revert, URL building, index parsing.
- Hardcoded base consts: repo `cavefish-dev/g13-driver`, branch `main`, dir `catalog`. URLs are
  `https://raw.githubusercontent.com/cavefish-dev/g13-driver/main/catalog/{index.json | <filename>}`.
- **Threading:** a background thread does all network work; a shared
  `catalog_state: Arc<Mutex<CatalogState>>` (`Idle | Loading | Loaded(Vec<CatalogEntry>) | Failed(String)`)
  is read by the Catalog tab — mirrors auto-update's `update_status`. The GUI never blocks; failures
  surface to a status line. Browsing is user-initiated (a Refresh button; no auto-fetch on launch).

## Schema: the `origin` field

Extends the provenance `[meta]` table with one optional field:

```toml
[meta]
name = "Gaming"
source = "github"
origin = "gaming.toml"   # catalog filename this was downloaded from
modified = true
```

- `origin` is present only for `source = github` profiles. `RawMeta` gains `origin: Option<String>`
  (`#[serde(default, skip_serializing_if = "Option::is_none")]`); `Profile` gains `origin:
  Option<String>` with `origin()`/`set_origin()`; `to_toml()` writes it only when `Some`. A `user`
  profile still serializes with zero provenance lines.
- **Set only** by `catalog::download`/`revert`. **`duplicate` clears `origin`** (a copy is the
  user's own artifact — provenance already resets to `user`); **`rename` preserves** it; edit/save
  doesn't touch it.
- **`profiles::list` / `ProfileEntry`** surface `origin` so the Catalog tab computes "Downloaded"
  status without re-reading files.

## Catalog core (`src/catalog.rs`)

**Pure helpers (unit-tested):**
- `struct CatalogEntry { filename: String, name: String }` (serde `Deserialize`).
- `parse_index(json: &str) -> anyhow::Result<Vec<CatalogEntry>>`.
- `index_url() -> String`, `profile_url(filename: &str) -> String` — built from the base consts.
- `mark_downloaded(entries: Vec<CatalogEntry>, local_origins: &HashSet<String>) -> Vec<(CatalogEntry, bool)>`
  — the Downloaded-status join; pure, no I/O.
- `stamp_download(profile: &mut Profile, origin: &str)` — set `source = Github`, `origin`,
  `modified = false` (keeps the upstream `[meta].name`). Shared by download and revert.

**Network + file ops (thin; manual-verify — same exception as auto-update's `fetch_latest_json`):**
- `fetch_index() -> Result<Vec<CatalogEntry>>` — `ureq` GET the index (User-Agent header),
  `parse_index`.
- `fetch_profile(filename: &str) -> Result<Profile>` — GET `catalog/<filename>`, parse via
  `Profile::from_raw` (the parse-validate gate — malformed content errors here).
- `download(dir: &Path, filename: &str) -> Result<String>` — `fetch_profile` → `stamp_download` →
  write under `unique_filename` → return the new local filename.
- `revert(path: &Path, origin: &str) -> Result<()>` — `fetch_profile(origin)` → `stamp_download` →
  overwrite `path` wholesale.

`download` and `revert` both funnel through `fetch_profile` so the validate gate and stamping are
shared, not duplicated. The GUI calls `download`/`revert` on a background thread, then `reload_now`.

## CI index generator & catalog seeding

- **`.github/workflows/catalog-index.yml`:** trigger on `push` to `main` with
  `paths: ['catalog/**.toml']` (NOT `index.json`, to avoid re-trigger loops) + `workflow_dispatch`.
  Job (ubuntu-latest, SHA-pinned actions): a step scans `catalog/*.toml`, reads each `[meta].name`
  (fallback to the filename stem) with Python stdlib `tomllib` (3.11+ preinstalled — no deps), writes
  a sorted `catalog/index.json` of `{filename, name}`, and commits it back only if changed
  (`git diff --quiet || …`) using the existing `contents: write` token.
- **Seed `catalog/`:** `catalog/gaming.toml` (repurpose the dropped `game.toml`, `name = "Gaming"`),
  `catalog/coding.toml` (IDE shortcuts, `name = "Coding"`), and a committed initial
  `catalog/index.json` (generated locally the same way) so the app can fetch immediately. Catalog
  files are plain profiles (no `source`/`origin`).

## UI: Catalog tab + Revert button

- **New `Catalog` tab** (tab set becomes Monitor / Profiles / Bindings / Catalog / LCD / Settings):
  a **Refresh** button spawns `fetch_index` on a thread; shows *Loading…* / *Failed: …* / the list.
  Each row: **name** + a **Download** button, disabled and labeled **"Downloaded"** when a local
  profile already carries that `origin`. Download → thread → `catalog::download` → `reload_now` →
  status "Downloaded <name>." Before the first Refresh: a hint *"Refresh to load the profile catalog
  from GitHub."* (no auto-fetch on launch).
- **Revert button — Bindings tab**, beside the provenance note; shown only when the active profile is
  `source == github` && `modified` && has an `origin`. Label **"Revert to GitHub version"**; a small
  confirm (*"Discard your changes and restore the downloaded version?"*) → thread →
  `catalog::revert(active_path, origin)` → `reload_now` → status. A github+modified profile with no
  `origin` shows no button.
- **State/threading:** `catalog_state: Arc<Mutex<CatalogState>>` on `MonitorApp` (mirrors
  `update_status`); `request_repaint()` on completion. Lock discipline per the hard-won rule:
  snapshot under a short lock; never hold a `profiles` lock across an egui closure or `reload_now`.

## Error handling

Every failure logs and surfaces without crashing or half-writing: offline / rate-limited / bad index
→ `Failed(msg)` + status line; a download that doesn't parse as a `Profile` → rejected at
`fetch_profile`, nothing written; revert fetch failure → local file untouched. No
`panic!`/`unwrap()`/`expect()` in the catalog or UI path (lock-poison `.unwrap()` excepted). Network
work is best-effort and never blocks the UI thread.

## Testing

- **Unit (TDD, pure logic):** `parse_index` (valid / empty / malformed→error); `index_url` /
  `profile_url` exact strings; `mark_downloaded` join (downloaded vs not vs empty origins);
  `stamp_download` (sets source/origin/modified=false, preserves upstream name); `Profile` `origin`
  parse + `to_toml` round-trip + omitted for user profiles; `duplicate` clears `origin`;
  `profiles::list`/`ProfileEntry` surface `origin`.
- **Manual-verify (documented exception — network + GUI):** live Refresh lists the seeded catalog;
  Download pulls a profile in and it flips to "Downloaded"; edit + **Revert** restores it and clears
  the "edited" badge; offline Refresh shows a clean error.
- **CI:** the index workflow verified by a real push adding a `catalog/*.toml` → `index.json`
  regenerates and commits.

## Dependencies

No new crates — reuses `ureq`, `serde_json`, `serde`, `toml`, `anyhow`, `eframe` already present.

## Out of scope (follow-ups)

- Upstream-change detection ("update available" for downloaded profiles) and continuous sync.
- Catalog descriptions/categories (`[meta].description`, index `description` column, browse grouping).
- Pretty `[meta].name` labels sourced live (the index already provides names; this is moot unless
  the index format is bypassed).
- Authenticated GitHub access / private catalogs; user-configurable catalog source.
