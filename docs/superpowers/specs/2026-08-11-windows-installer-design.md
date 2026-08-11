# Windows Installer (Inno Setup) — Design

- **Date:** 2026-08-11
- **Milestone:** `milestones/open/windows-installer.md` (new)
- **Status:** approved, ready for implementation plan

A per-user `setup.exe` for the G13 driver, built with Inno Setup and published
alongside the existing portable zip. Ships **unsigned** for now (code signing is a
separate effort).

## Constraints (why per-user)

- The app **auto-updates by replacing its own exe** (`self_replace` in
  `src/update/apply.rs`) and loads `config.toml` / `profiles/` **next to the exe**
  (`resolve_config_path` in `src/main.rs`).
- Therefore the install location must be **user-writable** so `self_replace` keeps
  working with no elevation. → **per-user install**, no UAC. A `Program Files` install
  would need admin *and* break auto-update; rejected.

## Install target & payload

- **Location:** `%LOCALAPPDATA%\Programs\g13-driver\` (Inno `PrivilegesRequired=lowest`,
  `DefaultDirName={localappdata}\Programs\g13-driver`).
- **Files** (same payload as the portable zip):
  - `g13-driver.exe` — always installed/overwritten (so upgrades update it).
  - `config.toml`, `profiles/*` — installed with **`onlyifdoesntexist`** so a returning
    user's GUI edits survive an upgrade.
  - `LICENSE`, `README.txt` (from `packaging/README.txt`).
- **Shortcuts:**
  - Start-menu shortcut to `g13-driver.exe` (uses the exe's **embedded "g13" icon**
    automatically — no separate `.ico` needed for shortcuts).
  - Optional Desktop shortcut (Inno `Tasks` checkbox, unchecked by default).
  - "Launch g13-driver now" checkbox on the finish page (`Flags: postinstall nowait`).
- **Uninstaller:** registered in Add/Remove Programs; removes the whole install dir
  (including `config.toml`/`profiles/` — clean per-user removal).
- **Autostart:** NOT handled by the installer — the app already has a "Launch at login"
  toggle in Settings (a registry Run key). Keeping one mechanism avoids divergence.

## Inno script

`packaging/g13-driver.iss`:
- `AppName`/`AppVerName`/`AppId` (a fixed GUID so upgrades/uninstall are tracked),
  `AppPublisher`, `AppVersion` injected at build time via `iscc /DAppVersion=<v>`
  (the script reads `{#AppVersion}` with a fallback default).
- `PrivilegesRequired=lowest`, `DefaultDirName={localappdata}\Programs\g13-driver`,
  `DisableProgramGroupPage=yes`, `OutputBaseFilename=g13-driver-v{#AppVersion}-setup`.
- `[Files]` for the payload (with the `onlyifdoesntexist` flags noted above).
- `[Icons]` for the Start-menu / optional Desktop shortcuts.
- `[Run]` postinstall launch entry.
- `SetupIconFile` set to a `g13.ico` when available (the CI step provides it — see
  below); optional, setup still builds without it.

## CI (`release.yml`)

On the existing Windows build job, after `Package bundle`:
- `choco install innosetup -y` (or use a pinned action) to get `ISCC.exe`.
- Provide the "g13" `.ico` for `SetupIconFile`: `build.rs` already generates `g13.ico`
  under its `OUT_DIR` during the release build; the step locates it
  (`find target/release/build -name g13.ico`) and copies it to `packaging/g13.ico`. If
  not found, proceed without a custom setup icon (non-fatal).
- Run `ISCC.exe /DAppVersion=$VERSION packaging/g13-driver.iss`, producing
  `g13-driver-v$VERSION-setup.exe`; compute its `.sha256`.
- Upload it as a build artifact so `publish` attaches it to the GitHub Release
  **alongside** the portable zip. Both remain available (installer = convenience,
  zip = portable/manual).

## Testing

- **Automated:** the `.iss` compiles in CI (a real gate — a broken script fails the
  release build). No unit tests (packaging/build code, per project policy).
- **Manual smoke:** run `setup.exe` → installs per-user with no UAC prompt; Start-menu
  shortcut shows the "g13" icon and launches the app; the config/profiles land in the
  install dir; re-running the installer preserves an edited `config.toml`; the app's
  auto-update still works from the installed location; uninstall removes the app.

## Out of scope

- **Code signing** (installer + exe stay unsigned; SmartScreen "unknown publisher" is
  expected and noted in `README.txt`) — separate cert proposal.
- All-users / `Program Files` install; MSI; per-machine service.
- Bundling a WinUSB/Zadig driver installer (still a documented manual step).
