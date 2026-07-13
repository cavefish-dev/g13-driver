# In-app auto-update — design

- **Status:** approved (design)
- **Date:** 2026-07-13
- **Scope:** The running app checks GitHub Releases for a newer version, and — on the user's click —
  downloads it, verifies its SHA-256, replaces its own exe, and restarts, preserving the user's
  config/profiles. Windows-only implementation today, structured so other OS/arch slot in later.
  Final MVP sub-project (#3 of 3).

## Motivation

Releases are published to GitHub (sub-project #2), but users have to notice and download them by
hand. Auto-update closes the loop: the app self-reports its version (`env!("G13_VERSION")`), so it
can compare against the latest release and update in place.

## Decisions (from brainstorming)

- **UX model:** background check on **every startup** + **notify**; the user applies with one click.
  Plus a manual **"Check for updates"** button. Not fully automatic (silently swapping an unsigned
  exe is intrusive).
- **What updates:** **replace `g13-driver.exe` only**; **preserve** `config.toml`/`profiles/` (user
  data). Refreshing LICENSE/README and adding-missing-profiles are deferred.
- **Restart:** **auto-restart** into the new exe after the swap.
- **Mechanism:** DIY from small crates (not the `self_update` crate) for control over our exact
  release layout and SHA-256 verification.
- **Multi-OS future:** auto-update is inherently OS/arch-specific (self-replace, archive format,
  which asset). Isolate OS code behind `#[cfg(...)]` like the injector; Windows now, extensible.

## Architecture

New **`src/update/`** module: a platform-agnostic core + an OS-gated apply step.

- **`update/mod.rs` (platform-agnostic):** the check + orchestration. Queries the GitHub Releases
  API, parses the latest release, compares its version to `env!("G13_VERSION")` via `semver`, and
  selects the asset for the current platform via a `PLATFORM` token (`"windows-x64"` today, derived
  from `cfg!(target_os)`/`cfg!(target_arch)` to match the CI matrix `name`). Testable pieces:
  `is_newer(latest, current) -> bool`, `asset_name(version) -> String`, release-JSON parsing +
  asset selection. No OS/Win32 types here.
- **`update/apply.rs` (OS-specific, `#[cfg(windows)]`):** download the zip + `.sha256`, verify the
  checksum, extract the exe from the nested folder, `self-replace` the running binary, relaunch. A
  future `#[cfg(target_os = "linux")]` sibling would handle `.tar.gz` + Unix replace. The core calls
  `apply::install(release) -> Result<()>` behind the cfg boundary. Platforms without an `apply`
  impl simply don't offer "Update now" (the check can still say a newer version exists).
- Runs on a **background thread**; reports state to the GUI via shared `UpdateStatus`
  (`Arc<Mutex<…>>`) that the Settings tab reads. The GUI never blocks on the network or the swap.

## Version check, GitHub API & asset selection (`update/mod.rs`)

- **API:** `GET https://api.github.com/repos/cavefish-dev/g13-driver/releases/latest`,
  unauthenticated (public repo), with `User-Agent: g13-driver/<version>` (GitHub requires a UA).
  `/releases/latest` returns the newest **full** release (ignores drafts/prereleases). Parse with
  `serde_json` into `{ tag_name, assets: [{ name, browser_download_url }] }`.
- **Compare:** strip leading `v` from `tag_name` → `semver::Version`; parse `env!("G13_VERSION")`
  likewise; offer update only if `latest > current` (strictly newer).
- **Asset selection:** find the asset named `g13-driver-<tag>-<PLATFORM>.zip` and its
  `<...>.zip.sha256` sibling. If this platform's asset isn't in the release, treat as "no applicable
  update". Forward-compatible with future multi-asset releases.
- **Rate limits / offline:** unauthenticated GitHub API (~60 req/hr/IP) far exceeds one check per
  startup. Any failure (offline, rate-limited, parse error) → **silent** (no popup, no indicator).

## Apply — download, verify, extract (`update/apply.rs`, `#[cfg(windows)]`)

On the user's click, on a background thread:

1. **Download to `std::env::temp_dir()`:** stream the `.zip` to a temp file (avoid holding it in
   memory) and fetch the `.zip.sha256`, via `ureq` (bundles **rustls** — pure-Rust TLS, clean on the
   GNU toolchain).
2. **Verify checksum first:** SHA-256 the downloaded zip with `sha2`; parse the expected hex from
   the `.sha256` file (`<hex>  <name>` → first field); compare case-insensitively. **Mismatch →
   abort** (delete temps, report error). Nothing is installed unless the hash matches CI's.
3. **Extract only the exe:** with the `zip` crate, find the entry
   `g13-driver-<tag>-<PLATFORM>/g13-driver.exe` (nested folder) and extract just it to temp as the
   staged new binary. Fallback: if that exact path isn't found, use the single `*.exe` entry in the
   archive. (Ignore config/profiles/LICENSE/README in the archive — exe-only update.)
4. Until the swap, the running exe and the user's `config.toml`/`profiles/` are **untouched**; any
   failure aborts cleanly with nothing changed.

## Apply — self-replace & restart

1. **Self-replace** via the `self-replace` crate: renames the running `g13-driver.exe` to a temp
   name (Windows permits renaming a running exe), moves the staged new exe into the original path,
   schedules the old for deletion. The path is now the new version; the old process keeps running
   from the renamed file.
2. **Relaunch:** spawn `std::env::current_exe()` (now the new exe) with a marker arg (e.g.
   `--updated`), then the old process **exits promptly**, dropping its single-instance mutex. To
   avoid the race where the new process checks the mutex before the old has exited (and wrongly sees
   "already running"), the marker makes the new process **retry the single-instance acquire for a
   few seconds** — bridging the handoff deterministically. Net effect: old exits → new acquires the
   mutex and shows its window; worst case the window blinks during restart.
3. **Preserve data:** `config.toml`/`profiles/` are never touched; the new version starts with the
   user's settings and persisted Active/Dry-run mode.
4. **Failure handling:** if self-replace fails (permissions, AV lock), abort and report; the
   original exe stays in place and runnable (no half-updated state).

The mutex handoff on restart is the trickiest part — flagged for care and for the smoke test.

## UX integration (Settings tab + background thread)

- Startup spawns a background check writing shared `UpdateStatus`:
  `Checking → UpToDate | Available { version } | Failed`, later `Downloading → Installing`.
- **Settings tab:** current version (`env!("G13_VERSION")`); **"Check for updates"** button (manual
  re-check); when newer exists, **"Update available: vX.Y.Z"** + **"Update now"** (runs
  `apply::install` on a thread; shows **"Updating…"**, then restart on success / short error on
  failure). A subtle "Update available" hint in the window. Tray notifications deferred.

## Error handling

Every failure path (offline, rate-limited, bad JSON, checksum mismatch, extract error, self-replace
lock) **logs a warning and leaves the app working on the current version** — no crash, no
half-update. Background check fails **silently**; manual check shows a brief "Couldn't check for
updates." Checksum mismatch always aborts before install. No `panic!`/`unwrap()` in the update path.

## Testing

- **Unit (TDD, pure logic, no network/disk):** `is_newer`/semver compare (older/newer/equal/
  prerelease); `asset_name(version)` == `g13-driver-v<ver>-windows-x64.zip`; the `PLATFORM` token;
  parsing the `.sha256` file (extract hex, ignore the filename field); parsing a **release-JSON
  fixture** → tag + correct platform asset selection; SHA-256 of known bytes == expected hex.
- **Manual-verify (documented exception):** the live download + `self-replace` + restart. **Smoke
  test:** with the current build installed, cut a higher-version release (bump `version.txt` +
  `Cargo.toml`, let CI publish), run the older build → it detects the update; **Update now**
  downloads, verifies the SHA-256, swaps the exe, restarts into the new version, with
  `config.toml`/`profiles/` preserved.

## Dependencies (new)

`ureq` (with rustls TLS), `serde_json`, `sha2`, `zip`, `self-replace`, `semver`. All build on the
GNU toolchain (ureq+rustls avoids the OpenSSL/native-tls pain).

## Out of scope (follow-ups)
- Fully-automatic (no-click) updates; tray/toast update notifications.
- Refreshing LICENSE/README and adding-missing-profiles on update.
- Non-Windows `apply` implementations (Linux `.tar.gz` + Unix replace) — enabled once those matrix
  rows exist and the top-level `compile_error!` is relaxed.
- Code-signing the exe (SmartScreen still warns on the new exe); `%APPDATA%` config location.
- Delta/partial updates; rollback.
