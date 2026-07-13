# In-app auto-update

- **Status:** finished — **live smoke test passed** (v0.1.0 → v0.1.1, 2026-07-13)
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

## Smoke test — PASSED 2026-07-13 (the documented manual-verify exception)
The live download + `self-replace` + restart has no unit tests by design. Verified end-to-end by
cutting `v0.1.1` and updating a running `v0.1.0` build staged in a user-writable folder:

- **Live check:** the `0.1.0` app queried GitHub and correctly reported "up to date" before the
  release, then "Update available: v0.1.1" after (via **Check for updates**, no restart needed).
- **Update now** downloaded the zip, verified its SHA-256, extracted the exe, self-replaced the
  running binary, and restarted — all from the GUI.
- **Byte-for-byte correct install:** the exe on disk after the update hashed identically to the
  CI-published release artifact (`sha256 4511f010…`); the `.sha256` gate matched the zip
  (`76ec4eac…`).
- **Restart handoff worked:** old process `exit(0)`, new `--updated` process reacquired the
  single-instance mutex and came up as `0.1.1` (new PID).
- **User data preserved:** `config.toml` / `profiles/` in the install folder were untouched; no
  leftover self-replace temp exe.

Not separately exercised (low-risk, left to natural coverage): checksum-mismatch abort and
read-only-location swap failure → `Failed`. The install folder was user-writable (the portable-app
common case).

## Follow-ups (deferred, from spec + final review)
- **I1 (final review):** the `--updated` child, on `Already` after the full 10 s wait, exits
  silently — a rare "neither-running" window (slow AV / quarantine). Harden by logging + still
  signalling. Fast follow-up only if the smoke test shows timing sensitivity.
- Cap `download_to_file` response size (belt-and-suspenders; hash still catches tampering).
- Non-Windows `apply` (Linux `.tar.gz` + Unix replace) once those matrix rows exist.
- Fully-automatic (no-click) updates; tray/toast update notifications.
- Refresh LICENSE/README + add-missing-profiles on update; code-signing (SmartScreen); `%APPDATA%`
  config location.
