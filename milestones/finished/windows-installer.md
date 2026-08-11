# Windows installer (Inno Setup)

- **Status:** finished (implementation) — verification pending
- **Target:** v0.2 (distribution)
- **Updated:** 2026-08-11

## Goal
A per-user `setup.exe` (Inno Setup) published alongside the portable zip. No UAC,
keeps auto-update working. Unsigned for now (code signing is a separate effort).

## Tasks
- [x] `packaging/g13-driver.iss` — per-user install to `%LOCALAPPDATA%\Programs\g13-driver`, payload = zip contents, config/profiles `onlyifdoesntexist`, Start-menu shortcut (exe's g13 icon), postinstall launch, uninstaller.
- [x] `release.yml`: install Inno Setup, source the g13 `.ico`, compile the `.iss` (version injected), publish `setup.exe` + `.sha256` as release assets.
- [x] Note the unsigned/SmartScreen caveat in `packaging/README.txt`.

## Acceptance
`setup.exe` installs per-user with no UAC, Start-menu shortcut shows the g13 icon and
launches the app, config/profiles preserved on re-install, auto-update still works,
uninstall removes the app. Both installer and zip are release assets.

## Notes
- Design: `docs/superpowers/specs/2026-08-11-windows-installer-design.md`.
- Follow-up: code-signing proposal (cert acquisition + CI signing).

## Smoke test (manual) — PENDING (not run before merge; run when convenient)
- [ ] Run the locally-built `g13-driver-v0.2.1-test-setup.exe`: installs to
      %LOCALAPPDATA%\Programs\g13-driver with NO UAC prompt.
- [ ] Start-menu shortcut shows the "g13" icon and launches the app.
- [ ] config.toml + profiles/ land in the install dir; the app runs from there.
- [ ] Re-run the installer over an edited config.toml -> the edit is preserved.
- [ ] Uninstall (Add/Remove Programs) removes the app.
- [ ] (next release) CI builds + publishes setup.exe alongside the zip.

> Merged with the `.iss` proven by a local ISCC compile (8.3MB setup.exe) and both
> code tasks reviewed (a release-blocking `find`/pipefail bug in the CI step was caught
> and fixed). Still to confirm: a manual install run, and the CI build at the next release.

## Code-signing decision (2026-08-11): STAY UNSIGNED
Deliberate — a personal OSS tool; the README documents the SmartScreen "unknown
publisher → Run anyway" workaround. Not an oversight. Revisit path if ever wanted:
**Azure Trusted Signing** (~$120/yr, cloud HSM, has a first-party GitHub Action) is the
CI-friendly choice; it drops into `release.yml` as a signing step after the build +
after ISCC, with no change to the installer/exe layout. Avoid hardware-token OV certs
for CI (USB token doesn't suit GitHub-hosted runners).
