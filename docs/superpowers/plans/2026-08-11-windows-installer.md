# Windows Installer (Inno Setup) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** A per-user `setup.exe` (Inno Setup) built + published alongside the portable zip; unsigned for now.

## Global Constraints
- **GNU toolchain** for the Rust build (unchanged). Inno Setup's `ISCC.exe` is a separate Windows tool (installed via `choco`/`winget`).
- Per-user install to `%LOCALAPPDATA%\Programs\g13-driver` (no UAC); keeps `self_replace` auto-update working. Do NOT install to Program Files.
- `config.toml`/`profiles/` install `onlyifdoesntexist` (preserve GUI edits on upgrade); the exe always updates.
- Unsigned — note the SmartScreen caveat in `README.txt`. Code signing is a separate effort.
- One focused commit per task.

## File Structure
- **Create** `packaging/g13-driver.iss` — the Inno Setup script.
- **Modify** `packaging/README.txt` — add the unsigned/SmartScreen + installer note.
- **Modify** `.github/workflows/release.yml` — build + publish the installer.
- **Modify** milestone.

---

## Task 1: `packaging/g13-driver.iss` + README note

**Files:** Create `packaging/g13-driver.iss`; Modify `packaging/README.txt`.

- [ ] **Step 1: Write the Inno script.** Create `packaging/g13-driver.iss`. `[Files]` sources are relative to the `.iss` dir (`packaging/`), so payload files are `..\`-prefixed:

```iss
; g13-driver per-user installer. Compile with:
;   ISCC.exe /DAppVersion=<version> packaging\g13-driver.iss
#ifndef AppVersion
  #define AppVersion "0.0.0-dev"
#endif

[Setup]
AppId={{7C4B2E90-4E1A-4C2E-9E7A-9F1D3B0A6C13}
AppName=g13-driver
AppVersion={#AppVersion}
AppVerName=g13-driver {#AppVersion}
AppPublisher=cavefish-dev
DefaultDirName={localappdata}\Programs\g13-driver
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputDir=..
OutputBaseFilename=g13-driver-v{#AppVersion}-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
#if FileExists(AddBackslash(SourcePath) + "g13.ico")
SetupIconFile=g13.ico
#endif

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Files]
Source: "..\target\release\g13-driver.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\config.toml"; DestDir: "{app}"; Flags: onlyifdoesntexist
Source: "..\profiles\*"; DestDir: "{app}\profiles"; Flags: onlyifdoesntexist recursesubdirs createallsubdirs
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "README.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\g13-driver"; Filename: "{app}\g13-driver.exe"
Name: "{autodesktop}\g13-driver"; Filename: "{app}\g13-driver.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\g13-driver.exe"; Description: "Launch g13-driver"; Flags: nowait postinstall skipifsilent
```

Notes: shortcuts point at the exe, which carries the **embedded "g13" icon**, so no `.ico` is needed for shortcuts. `SetupIconFile` is included only when `packaging/g13.ico` exists (CI provides it); the script compiles fine without it. `AppId` is a fixed GUID so upgrades/uninstall are tracked.

- [ ] **Step 2: README note.** Append to `packaging/README.txt` a short section: the app is currently **unsigned**, so Windows SmartScreen may show an "unknown publisher" prompt on first run of `setup.exe`/the app — choose "More info → Run anyway". Mention the installer is **per-user** (installs to your user profile, no admin needed) and auto-updates in place; the portable zip remains available.

- [ ] **Step 3: Verify by compiling locally.** Install Inno Setup on this machine if absent (`winget install --id JRSoftware.InnoSetup -e` or `choco install innosetup -y`), then build a release exe and compile the script:
  - `cargo build --release`
  - `"/c/Program Files (x86)/Inno Setup 6/ISCC.exe" "//DAppVersion=0.2.1-test" packaging/g13-driver.iss` (the `//D` double-slash avoids Git Bash mangling the `/D` flag; or run from PowerShell: `& 'C:\Program Files (x86)\Inno Setup 6\ISCC.exe' /DAppVersion=0.2.1-test packaging\g13-driver.iss`).
  - Confirm it produces `g13-driver-v0.2.1-test-setup.exe` in the repo root with no errors.
  - If Inno Setup genuinely can't be installed here, STOP and report — this task's verification is the local compile; do not fake it.

- [ ] **Step 4: Commit** (do NOT commit the generated `setup.exe` or `packaging/g13.ico` — add them to `.gitignore` if produced locally):

```bash
git add packaging/g13-driver.iss packaging/README.txt .gitignore
git commit -m "packaging: per-user Inno Setup installer script"
```

---

## Task 2: Build + publish the installer in `release.yml`

**Files:** Modify `.github/workflows/release.yml`.

- [ ] **Step 1: Add an installer step to the `build` job**, after "Package bundle + SHA256":

```yaml
      - name: Build installer (Inno Setup)
        shell: bash
        env:
          VERSION: ${{ needs.prepare.outputs.version }}
        run: |
          choco install innosetup -y --no-progress
          ISCC="/c/Program Files (x86)/Inno Setup 6/ISCC.exe"
          # Give the setup.exe the g13 icon that build.rs generated (best-effort).
          ICO="$(find target/release/build -name g13.ico | head -n1 || true)"
          if [ -n "$ICO" ]; then cp "$ICO" packaging/g13.ico; fi
          "$ISCC" "//DAppVersion=${VERSION}" packaging/g13-driver.iss
          SETUP="g13-driver-v${VERSION}-setup.exe"
          test -f "$SETUP"
          sha256sum "$SETUP" > "${SETUP}.sha256"
          ls -l "$SETUP" "${SETUP}.sha256"
```

- [ ] **Step 2: Include the installer in the upload glob.** In "Upload build artifacts", add the setup files to `path`:

```yaml
          path: |
            g13-driver-v*-*.zip
            g13-driver-v*-*.zip.sha256
            g13-driver-v*-setup.exe
            g13-driver-v*-setup.exe.sha256
```

- [ ] **Step 3: Attach the installer in `publish`.** In the "Create GitHub Release" `files:` list, add:

```yaml
            dist/*-setup.exe
            dist/*-setup.exe.sha256
```

- [ ] **Step 4: Sanity-check the YAML.** It's not runnable here without a release; verify indentation/syntax by eye and (if available) `python -c "import yaml,sys; yaml.safe_load(open('.github/workflows/release.yml'))"`. The real gate is the next release build.

- [ ] **Step 5: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci(release): build + publish the Inno Setup installer alongside the zip"
```

---

## Task 3: Milestone

- [ ] Set `milestones/open/windows-installer.md` `Status: ongoing`, check the implemented boxes, add a smoke checklist (run the locally-built `setup.exe`: installs per-user no-UAC; Start-menu shortcut shows the g13 icon + launches; config/profiles land in the install dir; re-install preserves an edited config; uninstall removes it). `git mv` to `milestones/ongoing/`. `cargo test && cargo build --release` clean. Commit `docs: windows-installer milestone to ongoing`.

---

## Self-Review
- `.iss` per-user + payload + onlyifdoesntexist + shortcut/uninstaller + postinstall launch → Task 1. ✓
- README unsigned/SmartScreen note → Task 1. ✓
- CI build + publish alongside zip → Task 2. ✓
- Local compile verification (real gate for the script); CI verified at next release; manual install smoke → Tasks 1/3. ✓
- Consistent: `OutputBaseFilename=g13-driver-v{#AppVersion}-setup`, `/DAppVersion`, upload/publish globs all reference `g13-driver-v*-setup.exe`.
