# Windows installer (Inno Setup)

- **Status:** open
- **Target:** v0.2 (distribution)
- **Updated:** 2026-08-11

## Goal
A per-user `setup.exe` (Inno Setup) published alongside the portable zip. No UAC,
keeps auto-update working. Unsigned for now (code signing is a separate effort).

## Tasks
- [ ] `packaging/g13-driver.iss` — per-user install to `%LOCALAPPDATA%\Programs\g13-driver`, payload = zip contents, config/profiles `onlyifdoesntexist`, Start-menu shortcut (exe's g13 icon), postinstall launch, uninstaller.
- [ ] `release.yml`: install Inno Setup, source the g13 `.ico`, compile the `.iss` (version injected), publish `setup.exe` + `.sha256` as release assets.
- [ ] Note the unsigned/SmartScreen caveat in `packaging/README.txt`.

## Acceptance
`setup.exe` installs per-user with no UAC, Start-menu shortcut shows the g13 icon and
launches the app, config/profiles preserved on re-install, auto-update still works,
uninstall removes the app. Both installer and zip are release assets.

## Notes
- Design: `docs/superpowers/specs/2026-08-11-windows-installer-design.md`.
- Follow-up: code-signing proposal (cert acquisition + CI signing).
