# CI + version.txt-driven releases

- **Status:** finished
- **Date:** 2026-07-12

## Outcome
GitHub Actions CI + release pipeline. Spec: `docs/superpowers/specs/2026-07-12-ci-releases-design.md`;
plan: `docs/superpowers/plans/2026-07-12-ci-releases.md`. MVP sub-project #2 of 3.
- `version.txt` (bare semver) is the single source of truth, stamped into the binary via `build.rs`
  (`env!("G13_VERSION")`); `ci/check-version.sh` fails the build if `Cargo.toml` disagrees.
- `ci.yml` builds + tests every push/PR (windows-latest, GNU toolchain, MinGW gcc, rust-cache),
  extensible matrix (one Windows row today).
- `release.yml` (on `version.txt` bump): prepare (guard + idempotency) → build (matrix, zip bundle +
  SHA256) → publish one full GitHub Release with auto-generated notes.
- Verified: green CI run; a real `vX.Y.Z` Release with the bundle + `.sha256` attached.

## Follow-ups
- Code-sign the exe (cert) — until then SmartScreen warns on first run.
- Enable non-Windows / cross-compiled matrix rows (needs the top-level `compile_error!` relaxed).
- Sub-project #3: auto-update pulling these Release artifacts (`env!("G13_VERSION")` vs latest
  Release, verify SHA256, apply without clobbering user config/profiles).
