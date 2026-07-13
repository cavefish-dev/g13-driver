# In-app auto-update

- **Status:** finished (code); **live smoke test pending** (needs a released v0.1.1 — see below)
- **Date:** 2026-07-13

## Outcome
The running app checks GitHub Releases on every startup and lets the user apply a newer version
with one click, preserving `config.toml`/`profiles/`. Spec:
`docs/superpowers/specs/2026-07-13-auto-update-design.md`; plan:
`docs/superpowers/plans/2026-07-13-auto-update.md`. Final MVP sub-project (#3 of 3).

- **`src/update/mod.rs`** (platform-agnostic): `PLATFORM` token (`"windows-x64"`), `asset_name`,
  `is_newer` (semver, strips leading `v`), release-JSON parse + asset selection (`select_update`),
  `UpdateStatus`, `fetch_latest_json`/`check` (GitHub `/releases/latest`, unauthenticated, UA header).
  No OS/Win32 types — a future Linux `apply` sibling slots in behind the same seam.
- **`src/update/apply.rs`** (`#![cfg(windows)]`): download zip + `.sha256` to `temp_dir()`,
  **verify SHA-256 before touching anything** (mismatch → bail, delete temp), extract the exe from
  the nested `g13-driver-vX.Y.Z-windows-x64/` folder (never joins archive paths to the FS — no
  zip-slip), `self-replace` the running binary, relaunch with `--updated`, then `exit(0)`.
- **Restart handoff:** the old process drops the single-instance mutex on exit; the `--updated`
  child calls `acquire_retry(10s)` (`src/single_instance.rs`), spinning on `Already` at 200 ms until
  the old process releases — no double-run of the GUI/injector.
- **UX** (`src/monitor/mod.rs` Settings tab): silent background check on startup; shows current
  version, "Update available: vX.Y.Z" + **Update now**, and a manual **Check for updates**. All
  failures (offline, rate-limited, bad JSON, checksum mismatch, self-replace lock) degrade to
  `Failed`/`Idle` — never a crash, never a half-update.
- **Deps** (GNU-toolchain clean): `ureq` (rustls+ring — confirmed building under MinGW, no
  native-tls fallback needed), `serde_json`, `semver` in `[dependencies]`; `sha2`, `zip`,
  `self-replace` in `[target.'cfg(windows)'.dependencies]`.

Built via subagent-driven-development (6 tasks); 114 unit tests pass. Final whole-branch review
(opus, adversarial): no Critical — integrity gate correctly ordered before install, no zip-slip,
race-free restart, all failure paths non-crashing. **Merge-after-smoke-test** recommendation.

## Smoke test (the documented manual-verify exception — do this once a v0.1.1 exists)
The live download + `self-replace` + restart has no unit tests by design. Steps:
1. Build the current `0.1.0` release, stage exe + `config.toml` + `profiles/` to a **user-writable**
   install folder; run it.
2. Cut `v0.1.1`: bump `version.txt` + `Cargo.toml` on `main` → CI publishes the release.
3. In the running `0.1.0` app: Settings → confirm "Update available: v0.1.1" → **Update now** →
   it downloads, verifies the SHA-256, swaps the exe, restarts into `0.1.1`.
4. Confirm: `config.toml`/`profiles/` preserved; the 10 s `--updated` reacquire succeeds; a
   checksum mismatch aborts with the old exe intact; a swap from a read-only location surfaces
   `Failed` rather than corrupting the install.

## Follow-ups (deferred, from spec + final review)
- **I1 (final review):** the `--updated` child, on `Already` after the full 10 s wait, exits
  silently — a rare "neither-running" window (slow AV / quarantine). Harden by logging + still
  signalling. Fast follow-up only if the smoke test shows timing sensitivity.
- Cap `download_to_file` response size (belt-and-suspenders; hash still catches tampering).
- Non-Windows `apply` (Linux `.tar.gz` + Unix replace) once those matrix rows exist.
- Fully-automatic (no-click) updates; tray/toast update notifications.
- Refresh LICENSE/README + add-missing-profiles on update; code-signing (SmartScreen); `%APPDATA%`
  config location.
