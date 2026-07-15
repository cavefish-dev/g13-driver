# Profile catalog (download / revert from GitHub)

- **Status:** finished — GUI + live-network smoke-tested (2026-07-15)
- **Date:** 2026-07-15

## Outcome
Browse a curated GitHub catalog of profiles, download them into the local library, and revert an
edited GitHub-sourced profile to upstream. Spec:
`docs/superpowers/specs/2026-07-15-profile-catalog-design.md`; plan:
`docs/superpowers/plans/2026-07-15-profile-catalog.md`.

- **`src/catalog.rs`** (platform-agnostic — `ureq` + file writes + TOML, no `#[cfg]`): fetches the
  CI-generated `catalog/index.json` from `raw.githubusercontent.com` (`[{filename,name}]`), downloads
  a catalog profile (validates it parses as a `Profile` — the integrity gate; no SHA since it's data,
  not a binary), and reverts by re-fetching. `download` stamps `source=github`/`origin`/`modified=false`
  and writes under `unique_filename` (no clobber); `revert` fetches+validates BEFORE overwriting, so a
  bad fetch can't corrupt the local file. Hardcoded base: `cavefish-dev/g13-driver` @ `main`, `catalog/`.
- **`[meta].origin`** schema field (catalog filename) added — the upstream identity deferred from the
  provenance feature. Stamped on download/revert; serialized only for `source=github`; `duplicate`
  clears it, `rename` preserves it; surfaced through `profiles::list`/`ProfileEntry`.
- **Catalog tab** (`src/monitor/mod.rs`): Refresh (background `fetch_index`), rows with name +
  Download / "Downloaded" (dedup by `origin` join against local profiles). **Revert to GitHub
  version** button on the Bindings tab (github + modified + has origin). All network on background
  threads with a shared `catalog_status`; lock discipline verified deadlock-free.
- **CI:** `.github/workflows/catalog-index.yml` regenerates `catalog/index.json` from `catalog/*.toml`
  via Python `tomllib` and commits it back through the GitHub API (verified signature). Seeded
  `catalog/gaming.toml` + `catalog/coding.toml` + `index.json`.

Built via subagent-driven-development (7 tasks), 161 unit tests. Per-task reviews + a final
whole-branch review (opus): MERGE, no Critical/Important (deadlock-free whole-GUI, integrity gate
confirmed, `origin` correct end-to-end, URLs exact).

## Smoke test — PASSED 2026-07-15 (post-merge, live network)
The catalog is fetched from `main`, so the live test ran after merge+push published `catalog/`.
Verified: Catalog → Refresh lists Coding + Gaming; Download pulls Gaming into the library (GitHub
badge, row → "Downloaded"); edit + Save flips it to "GitHub · edited"; Revert restores upstream.

## Post-merge fixes & follow-ups
- **CI SHA typo (fixed, commit 2fde138):** the `actions/github-script` pin in the plan was wrong
  (`…cdc9` vs the real `…cdea`); the first workflow run failed to resolve the action. Corrected.
- **Branch-ruleset bypass (pending user action — option A chosen):** the workflow's API commit to
  `main` is blocked by the ruleset ("Cannot update this protected ref"). Fix: add a **GitHub Actions**
  bypass actor to the main ruleset (Settings → Rules → Rulesets → Bypass list). The feature works
  without it (seeded `index.json` is live); only *auto-regeneration* of the index needs it. Also
  normalize the seeded `index.json` to the generator's exact `json.dumps(indent=2)` format so the
  first allowed run is a clean no-op.
- **Bindings action-bar visibility (fixed, commit d8246c5):** found during the catalog smoke test —
  the Save / Revert buttons rendered below a fixed-height scroll area and were clipped on a short
  window. Pinned the action controls to the bottom via `TopBottomPanel::bottom(...).show_inside`;
  the binding list now fills the space above and scrolls. Renamed the buffer-revert button to
  "Revert edits" to disambiguate from "Revert to GitHub version".
- Deferred (spec out-of-scope): upstream-change detection / continuous sync; catalog
  descriptions/categories; authenticated/private catalogs.
