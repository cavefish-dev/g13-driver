# Profile provenance metadata — design

- **Status:** approved (design)
- **Date:** 2026-07-14
- **Scope:** Groundwork for the future "download profiles from GitHub" feature: mark each profile
  with its origin (`github` vs `user`) and whether a GitHub-sourced profile has been edited, and
  surface that state in the GUI. This is *state tracking only* — the download and revert **actions**
  are deferred to the GitHub-download feature, which will produce the upstream versions to revert to.

## Motivation

The just-merged profile-management feature made profiles a folder-based library with a `[meta].name`
field. Users will soon download curated profiles from the project's GitHub repo. To support that —
and specifically to let users later **revert** an edited download back to its GitHub version — each
profile needs to record where it came from and whether it still matches upstream. This spec adds
that metadata and its state transitions now, so the download feature has a clean foundation.

## Decisions (from brainstorming)

- **Scope A (groundwork only):** schema + state tracking + GUI display now; download/revert actions
  ride in with the GitHub-download feature.
- **Edited is an explicit flag, not a checksum.** A boolean `modified` flipped on in-app save —
  chosen over a content hash because profiles are freely added to the repo and per-file checksums
  would add contributor friction. Trade-off accepted: hand edits to the `.toml` outside the app do
  not flip the flag.
- **Bundled profiles are `github`** (choice B): the shipped `basic`/`media` are marked
  `source = "github"`, so they badge as GitHub and become "edited" when changed.
- **Upstream identity is deferred.** Nothing here records *which* repo file a profile came from; the
  download feature adds that field when it exists. No `github`-sourced profiles are created locally
  by this task except the two bundled ones.

## Schema & serialization

Two new optional fields in the existing `[meta]` table:

```toml
[meta]
name = "Basic"
source = "github"   # "github" | "user"
modified = true     # only meaningful when source = "github"
```

- **`source`** — provenance. Absent ⇒ `"user"`. An unknown/garbage string ⇒ `User` (defensive).
- **`modified`** — absent ⇒ `false`. Meaningful only when `source = "github"`.
- **Serialization is minimal:** `to_toml()` writes `source` only when `Github`, and `modified` only
  when `true`. A `user`/unmodified profile serializes with **no** `source`/`modified` lines (keeps
  user files clean, matching how empty `name` is omitted).
- **Types:** `RawMeta` gains `source: Option<String>`, `modified: Option<bool>`. `Profile` gains
  `source: ProfileSource` (enum `User | Github`) and `modified: bool`. `Profile` exposes read
  accessors (`source()`, `modified()`) and setters as needed by the transitions below.

## State transitions (who sets what)

`modified` in this task only ever flips **false → true** (the save path); it returns to `false`
only via the future revert.

| Action | `source` | `modified` |
|--------|----------|-----------|
| **New** (`create`) | `user` (omitted) | `false` (omitted) |
| **Duplicate** | `user` — **provenance resets** (a copy is the user's own artifact) | `false` |
| **Rename** | preserved untouched (name-only edit is not a content edit) | preserved untouched |
| **Edit & save** (Bindings tab) | unchanged | set `true` **iff** current `source == github` |
| **Download** (future) | `github` | `false` |
| **Revert** (future) | `github` | `false` |

- **Rename** already edits only `[meta].name` via `toml_edit`, so `source`/`modified` survive with no
  extra work — a test locks this in.
- **Edit & save** lives in `ProfileSet::save_active_bindings` (which rewrites the file via
  `Profile::to_toml`): before serializing, if the profile's `source == Github`, set `modified = true`.
  Saving a `user` profile changes nothing provenance-wise. Unit-testable without the GUI.

## GUI display (`src/monitor/mod.rs`)

Display-only; nothing here changes assignment, deletion, or folder mechanics.

- `profiles::list` resolves `source`/`modified` into `ProfileEntry` (new fields
  `source: ProfileSource`, `modified: bool`) so the UI needs no re-read.
- **Library rows** — a small trailing badge after the display name, via existing
  `ui.weak(...)`/small-label styling:
  - `user` → no badge (your own profiles stay visually quiet).
  - `github`, unmodified → **"GitHub"**.
  - `github`, modified → **"GitHub · edited"**.
- **Bindings tab** — when editing a `github` profile, a one-line informative note: *"From GitHub —
  your edits will mark this profile as edited"*, changing to note *"edited"* once it is. No revert
  button yet (that's the download feature).

## Bundled profiles

- `profiles/basic.toml` and `profiles/media.toml` each get `source = "github"` added to their
  `[meta]` table (no `modified` line — they ship unmodified). This is the only change to shipped
  content. `config.toml` and the rest of the bundle are unchanged.

## Error handling

Consistent with the project policy: parsing is defensive (absent/garbage `source` ⇒ `User`,
absent `modified` ⇒ `false`); the save-path `modified` flip is best-effort within the existing
`save_active_bindings` `Result` (a write failure surfaces to the Bindings status line as today). No
`panic!`/`unwrap()` on profile data. Badges never fail — worst case a mis-tagged badge, never a crash.

## Testing

- **Unit (TDD, pure logic):**
  - `config.rs`: parse `source` (`github` / `user` / absent→User / garbage→User) and `modified`
    (`true` / absent→false); `to_toml()` round-trips both and **omits** them for the user/unmodified
    default (a plain user profile has no `source`/`modified` lines); a `github`+`modified` profile
    round-trips.
  - `profiles.rs`: `create` ⇒ `source == User`; `duplicate` of a `github` profile ⇒
    `source == User, modified == false`; `rename` of a `github` profile preserves `source`/`modified`;
    `list` surfaces `source`/`modified` into `ProfileEntry`.
  - Edit-save flip: `save_active_bindings` on a `github` profile writes `modified = true` to disk; on
    a `user` profile the file stays clean (no `source`/`modified`).
- **Manual-verify (GUI, documented exception):** library badges render (`GitHub` / `GitHub · edited`
  / none) and the Bindings note appears for a GitHub profile.

## Out of scope (follow-ups)

- The GitHub-download feature itself: browsing/fetching profiles, the upstream-identity field, and
  the **Revert** action (restore content + set `modified = false`).
- "Restore bundled defaults" as a distinct mechanism (bundled profiles are treated as `github` here).
- Detecting out-of-app hand edits to `.toml` files (the flag doesn't catch these, by choice).
