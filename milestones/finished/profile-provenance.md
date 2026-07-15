# Profile provenance metadata

- **Status:** finished — GUI smoke-tested (2026-07-14)
- **Date:** 2026-07-14

## Outcome
Groundwork for the future GitHub-download/revert feature: each profile records its origin and edited
state. Spec: `docs/superpowers/specs/2026-07-14-profile-provenance-design.md`; plan:
`docs/superpowers/plans/2026-07-14-profile-provenance.md`.

- **Schema:** `[meta]` gains `source` (→ `ProfileSource { User, Github }`) and `modified` (bool).
  Serialization is minimal — `source` written only for `Github`, `modified` only when `true`; a
  plain user/unmodified profile emits no provenance lines (and no empty `[meta]`). Parsing is
  defensive: absent/garbage `source` ⇒ `User`, absent `modified` ⇒ `false`. Legacy `[meta]`-less
  files still load.
- **Transitions:** create ⇒ `user`; **duplicate ⇒ resets to `user`/unmodified** (a copy is the
  user's own artifact); **rename preserves** `source`/`modified` (only `[meta].name` changes);
  **edit-save flips `modified = true`** iff `source == github` (in `save_active_bindings`). In this
  feature `modified` only ever goes false→true — reset-to-false is the deferred revert.
- **GUI:** the Profiles library shows a badge — **GitHub** / **GitHub · edited** / none (user); the
  Bindings tab shows a "From GitHub…" note (reads provenance into locals under a bounded lock).
- **Bundled profiles:** `basic.toml`/`media.toml` ship with `source = "github"`.

Built via subagent-driven-development (5 tasks), 148 unit tests. Per-task reviews plus a final
whole-branch review (opus): **MERGE, no Critical/Important** — serialization verified (user profiles
never get provenance lines, no empty `[meta]` leak), transitions correct end-to-end, defensive
parsing, GUI lock-safe.

## Smoke test — PASSED 2026-07-14
Verified live: `basic`/`media` show the **GitHub** badge; editing + saving a GitHub profile flips it
to **GitHub · edited**; duplicate produces an unbadged (user) copy.

**Bringup note learned during the smoke test:** the app resolves `config.toml`/`profiles/` **beside
the exe first** (`resolve_config_path`), so a dev run uses `target/release/config.toml` +
`target/release/profiles/`, not the repo copies. A stale `target/release/` bundle (old
`default`/`game`/`media`, no `[meta]`) masked the change until it was synced to the repo bundle.
GUI edits during a smoke test land beside the exe, not in the repo (so they don't show as git changes).

## Follow-ups (deferred — the GitHub-download feature)
- Browse/fetch profiles from GitHub; the upstream-identity field (which repo file a profile came
  from); the **Revert** action (restore content + set `modified = false`).
- "Restore bundled defaults" as a distinct mechanism (bundled profiles are treated as `github` here).
- Detecting out-of-app hand edits to `.toml` (the flag doesn't catch these, by design).
- Minor: `read_entry_meta` implicitly depends on `ProfileSource::Default == User`; a redundant
  (harmless) second bounded read-lock in `render_bindings`.
