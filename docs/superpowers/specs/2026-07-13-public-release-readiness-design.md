# Public-release readiness (README + hardening) — design

- **Status:** approved (design)
- **Date:** 2026-07-13
- **Scope:** Everything needed to make `cavefish-dev/g13-driver` a good public open-source repo:
  a detailed, beginner-friendly `README.md`; a `docs/configuration.md` reference; supply-chain
  hardening (SHA-pinned Actions + Dependabot); community-health files (CONTRIBUTING, SECURITY,
  issue/PR templates); and repo metadata + the visibility flip. Last step before going public;
  sub-project #3 (auto-update) comes after.

## Motivation

The repo has working code, CI, and a v0.1.0 release, but no front door: no `README.md`, no
contributor/security docs, floating (hijackable) Action tags, and it is still private. Before
inviting the public — especially non-technical users who just want to download and run — it needs
approachable docs and basic supply-chain hygiene.

## Audience

- **Primary:** non-technical Windows users who download the release zip and run it. They configure
  via the **GUI**, not by editing TOML. The README's first half must get them running without
  jargon.
- **Secondary:** developers who build from source and may contribute.

## Components

### 1. `README.md` (new, repo root) — GUI-first, users-first

Sections in order:
1. Title + one-line description + badges (CI status, latest release, license).
2. What it does / why (remap G-keys + thumb buttons, joystick→WASD, M-key profiles, tray
   background app, auto-start) and what it replaces (abandonware official driver).
3. Requirements (Windows 10/11, a Logitech G13, ~5 min one-time setup).
4. **Quick start** (the core path): (a) download the latest release zip from Releases → extract
   the whole folder, keep files together; (b) one-time **Zadig/WinUSB** setup — short why + key
   steps, link `docs/zadig-setup.md`; (c) first run — double-click `g13-driver.exe`, the
   **SmartScreen** "Windows protected your PC → More info → Run anyway" note in plain language
   (unsigned, not malicious, signing planned); (d) it starts in **Dry-run** (safe), flip to
   **Active** to inject.
5. Using it — tray/background behavior (close/minimize → tray, Quit exits, red/green/grey icon),
   Dry-run vs Active, auto-start toggle.
6. Configuring — GUI-first (Bindings tab, Profiles M1/M2/M3, joystick, auto-repeat checkbox), one
   paragraph + link to `docs/configuration.md` for TOML/power-user detail.
7. Updating — download the newer release zip for now; note auto-update is coming.
8. Troubleshooting — short FAQ table (red/not-connected → Zadig/cable; keys not injecting →
   Dry-run; SmartScreen).
9. Building from source — GNU toolchain + MinGW gcc, `cargo build --release`, `cargo test`; link
   CONTRIBUTING.md.
10. Contributing / License (GPL-3.0-or-later) / Acknowledgements (generic credit to prior G13
    reverse-engineering).
11. **Roadmap** — "done ✓ / next / later" from milestones + CLAUDE.md: done (key/thumb mapping,
    joystick→WASD, profiles, GUI monitor + bindings editor, hold-means-hold + media keys,
    auto-repeat, tray app, CI+releases); next (auto-update); later (macros + shell commands, LCD
    160×43, RGB backlight, Linux + GUI configurator). Framed as direction, not promises.

Tone: plain, friendly, imperative; short sentences; no jargon in the user-facing half.

### 2. `docs/configuration.md` (new) — power-user reference

- Where config lives: `config.toml` (manifest) + `profiles/*.toml` next to the exe; resolved
  exe-dir-first then CWD; hot-reloaded on save.
- The manifest: `profiles_dir`, `m1`/`m2`/`m3` → profile files, `[autorepeat]` (`delay_ms`,
  `interval_ms`), `[app] start_active`.
- A profile file: `[keys]` bindings; key/combo syntax (modifiers `ctrl`/`shift`/`alt`/`win` + one
  key; modifier-only allowed); bindable inputs (G1–G22, BTN1/BTN2/STICK); valid key names (a-z,
  0-9, f1–f24, enter/esc/space/tab, arrows, home/end/pageup/pagedown, insert/delete, and the
  tap-only media keys playpause/nexttrack/prevtrack/volup/voldown/mute); `[joystick]` WASD
  (mode/deadzone/up/down/left/right); `[repeat]` per-binding auto-repeat.
- GUI vs hand-editing: the Bindings tab is the easy path; a GUI save rewrites the file (comments
  not preserved — known limitation).
- A small worked example.

Source of truth for these facts: `config.toml` comments, the Bindings-tab help text in
`src/monitor/mod.rs`, `src/injector/key_map.rs`, and the feature specs. The reference must match
the code, not invent names — the writer verifies against `key_map.rs`/`protocol.rs`/`config.rs`.

### 3. Hardening

- **SHA-pin Actions** in `.github/workflows/ci.yml` and `release.yml`: pin every `uses:` to a full
  commit SHA with a trailing `# vX.Y.Z` comment. Includes third-party (`softprops/action-gh-release`,
  `Swatinem/rust-cache`, `msys2/setup-msys2`) and GitHub-owned (`actions/checkout`,
  `actions/upload-artifact`, `actions/download-artifact`). SHAs resolved at implementation time via
  the GitHub API (the commit each current tag points to).
- **`.github/dependabot.yml`** — `github-actions` ecosystem, weekly, so it auto-PRs SHA bumps.
  (`cargo` ecosystem intentionally omitted for now.)
- **Re-verify:** after pinning, a CI run must stay green (confirmed via `gh` — changing action refs
  can break resolution).

### 4. Community-health files + metadata

- **`CONTRIBUTING.md`** — build (GNU toolchain + MinGW, PATH bits), `cargo test`, the
  spec→plan→milestone workflow (`docs/superpowers/` + `milestones/`), TDD expectation, commit style,
  GPL **inbound=outbound**.
- **`SECURITY.md`** — supported version = latest release; report privately via GitHub "Report a
  vulnerability" (private advisories), not a public issue; best-effort hobby-project response note.
- **`.github/ISSUE_TEMPLATE/bug_report.md`** (Windows version, G13 connection/Zadig done?,
  Dry-run/Active, steps, `RUST_LOG=debug` logs), **`feature_request.md`**, **`config.yml`** (disable
  blank issues, link docs), **`.github/pull_request_template.md`** (what/why, tests run, milestone
  link).
- **Repo metadata** via `gh repo edit`: description ("Open-source replacement driver for the
  Logitech G13 keypad (Windows) — remap keys, joystick→WASD, profiles, tray app") and topics
  (`logitech-g13`, `g13`, `keypad`, `rust`, `windows`, `driver`, `hid`, `libusb`, `gaming`, `egui`).

## Rollout order (safety)

1. Land all files (README, docs/configuration.md, CONTRIBUTING, SECURITY, templates, dependabot)
   and the SHA-pinning on `main`.
2. Re-verify CI is green.
3. **Flip visibility to public** (`gh repo edit --visibility public`) — the point of no return,
   done **only on the user's explicit go**.
4. Set description + topics.

## Testing

No application code changes, so no unit tests. Gates:
- **Docs accuracy (manual-verify):** the `docs/configuration.md` key names / syntax must match
  `src/injector/key_map.rs` + `src/protocol.rs` + `src/config.rs` — the writer cross-checks against
  those files (not from memory). README links resolve; commands (`cargo build --release`,
  `cargo test`) are correct.
- **Hardening (real-run):** after SHA-pinning, a CI run stays green (controller confirms via `gh`).
- **Markdown/YAML validity:** `dependabot.yml` is valid YAML; templates render.
- The visibility flip + metadata are manual controller steps, done last on user go.

## Out of scope (follow-ups / deferred)
- Screenshots in the README (text-only for now).
- Code-signing the exe (SmartScreen warning documented instead).
- `CODE_OF_CONDUCT.md`.
- `cargo` Dependabot ecosystem.
- Sub-project #3: auto-update.
