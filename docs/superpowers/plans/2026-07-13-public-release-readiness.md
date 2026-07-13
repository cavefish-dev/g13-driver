# Public-release readiness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `cavefish-dev/g13-driver` a good public repo — a beginner-friendly README, a config reference, SHA-pinned Actions + Dependabot, community-health files, and the metadata/visibility flip (last, on explicit user go).

**Architecture:** Documentation + config-as-code only; no application code changes. Files land on `main`, CI is re-verified green after the Action pinning, then the repo is flipped public.

**Tech Stack:** Markdown, GitHub Actions YAML, Dependabot, `gh` CLI.

Full design: `docs/superpowers/specs/2026-07-13-public-release-readiness-design.md`.

## Global Constraints

- No application code changes — this is docs + repo config. Existing tests (107) must stay green; nothing here touches Rust source.
- **Key names / config facts must match the code**, not be invented. Authoritative sources: `src/injector/key_map.rs` (valid key names), `src/protocol.rs` (`G13Key` variants), `src/config.rs` (manifest/profile parsing). The valid key names are enumerated in this plan (Task 2) from `key_map.rs` — use them verbatim.
- **License:** GPL-3.0-or-later. Contributions are inbound=outbound GPL.
- **Repo:** `cavefish-dev/g13-driver`; latest release `v0.1.0`; Windows-only; GNU toolchain (`x86_64-pc-windows-gnu`) + MinGW gcc.
- **Action SHA-pins** (resolved from the current moving tags — identical behavior to the green CI; do NOT upgrade majors here):
  - `actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5 # v4`
  - `actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02 # v4`
  - `actions/download-artifact@d3f86a106a0bac45b974a628896c90dbdf5c8093 # v4`
  - `Swatinem/rust-cache@e18b497796c12c097a38f9edb9d0641fb99eee32 # v2`
  - `msys2/setup-msys2@66cd2cce69caa17b53920067426061ca1de3a884 # v2`
  - `softprops/action-gh-release@3bb12739c298aeb8a4eeaf626c5b8d85266b0e65 # v2`
- **Visibility flip is the point of no return** — done only on the user's explicit go (Task 6), after everything else is on `main` and CI is green.
- One focused commit per task; imperative subject; end each commit message with:
  `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`

---

### Task 1: `README.md`

**Files:**
- Create: `README.md` (repo root)

**Interfaces:** Links to `docs/zadig-setup.md`, `docs/configuration.md` (Task 2), `CONTRIBUTING.md` (Task 4), `LICENSE`, and the Releases page.

- [ ] **Step 1: Create `README.md`** with exactly this content:

````markdown
# g13-driver

[![CI](https://github.com/cavefish-dev/g13-driver/actions/workflows/ci.yml/badge.svg)](https://github.com/cavefish-dev/g13-driver/actions/workflows/ci.yml)
[![Latest release](https://img.shields.io/github/v/release/cavefish-dev/g13-driver)](https://github.com/cavefish-dev/g13-driver/releases/latest)
[![License: GPL-3.0-or-later](https://img.shields.io/badge/license-GPL--3.0--or--later-blue)](LICENSE)

An open-source replacement driver for the **Logitech G13** gaming keypad on Windows. Logitech's
official software is abandonware; this brings the G13 back to life with a small, modern app.

## What it does

- Remap the **G-keys** (G1–G22), the two **thumb buttons**, and the **joystick click** to any key
  or keyboard shortcut (e.g. `ctrl+c`, `alt+tab`).
- Map the **joystick** to WASD (hold-to-move).
- **Profiles** on the M1/M2/M3 keys — switch whole binding sets on the fly.
- Runs quietly in the **system tray**; optional **auto-start at login**.
- Configure everything in a simple GUI — no config files required.

## Requirements

- Windows 10 or 11
- A Logitech G13 keypad
- About 5 minutes for a one-time driver setup

## Quick start

### 1. Download

Grab the latest `g13-driver-vX.Y.Z-windows-x64.zip` from the
[**Releases**](https://github.com/cavefish-dev/g13-driver/releases/latest) page and **extract the
whole folder**. Keep `g13-driver.exe`, `config.toml`, and the `profiles/` folder together.

### 2. One-time driver setup (Zadig)

Windows needs to let this app talk to the G13 over USB. You do this once with a free tool called
**Zadig**, which installs the generic **WinUSB** driver on the G13. It takes a minute and is
reversible.

Follow the step-by-step guide: **[docs/zadig-setup.md](docs/zadig-setup.md)**.

> This does not delete Logitech's software — it just points the G13 at WinUSB so g13-driver can read
> it. You can switch back anytime.

### 3. Run it

Double-click **`g13-driver.exe`**.

Because the app isn't code-signed yet, Windows **SmartScreen** may show *"Windows protected your
PC"*. Click **More info → Run anyway**. This is expected for a small open-source app; the code is
public in this repo and signing is on the roadmap.

### 4. Turn it on

The app starts in **Dry-run** mode — it shows what you press but injects nothing (safe for testing).
When you're ready, switch to **Active** (top-right toggle, or the tray menu) and your bindings take
effect.

## Using it

- **Tray app:** closing or minimizing the window **hides it to the tray** — the driver keeps
  running. Use **Quit** in the tray menu to actually exit.
- **Status icon:** green = Active, grey = Dry-run, **red = the G13 isn't connected** (run Zadig / check
  the cable). It auto-reconnects when you plug the G13 back in.
- **Auto-start at login:** enable it in **Settings** (or the tray menu) so the driver is ready every
  time you log in.

## Configuring

Everything is done in the GUI — you never have to touch a file:

- **Bindings tab:** click a key row, type the key or shortcut (e.g. `ctrl+c`), tick **repeat** to make
  it auto-repeat while held, then **Save**.
- **Profiles tab:** M1/M2/M3 each load a profile; press the M-key (or click the slot) to switch.
- **Joystick / auto-repeat:** joystick→WASD and repeat timing are configurable too.

Power users can hand-edit the TOML config files next to the exe — see
**[docs/configuration.md](docs/configuration.md)** for the full reference.

## Updating

For now, download the newer release zip and replace your files (keep your edited `config.toml` /
`profiles/` if you customized them). Built-in auto-update is on the roadmap.

## Troubleshooting

| Problem | Fix |
|---|---|
| Tray icon is **red** / "not connected" | Run the [Zadig setup](docs/zadig-setup.md); check the USB cable. |
| Keys do nothing | You're in **Dry-run** — switch to **Active**. |
| "Windows protected your PC" | SmartScreen on an unsigned app — **More info → Run anyway**. |
| A binding won't save | The key name is invalid — see [configuration.md](docs/configuration.md) for valid names. |

## Building from source

Windows, with the **GNU** Rust toolchain (not MSVC) and a MinGW-w64 `gcc` (for `rusb`'s bundled
libusb):

```sh
rustup default stable-x86_64-pc-windows-gnu
cargo test
cargo build --release   # -> target/release/g13-driver.exe
```

See **[CONTRIBUTING.md](CONTRIBUTING.md)** for the full developer setup and workflow.

## Roadmap

Roughly where things are headed (direction, not promises):

- **Done:** key/thumb/stick remapping, joystick→WASD, M-key profiles, GUI monitor + bindings editor,
  hold-means-hold + media keys, auto-repeat, tray background app, CI + GitHub releases.
- **Next:** in-app auto-update from GitHub Releases.
- **Later:** macros + shell commands, the G13 LCD (160×43), RGB backlight, Linux support, and a
  standalone GUI configurator.

## Contributing & license

Contributions welcome — see [CONTRIBUTING.md](CONTRIBUTING.md). Licensed under
**GPL-3.0-or-later** (see [LICENSE](LICENSE)); contributions are accepted under the same license.

Thanks to the broader community whose G13 reverse-engineering made an open driver possible.
````

- [ ] **Step 2: Verify links resolve**

Run: `for f in docs/zadig-setup.md LICENSE; do test -f "$f" && echo "ok $f" || echo "MISSING $f"; done`
Expected: `ok docs/zadig-setup.md`, `ok LICENSE`. (`docs/configuration.md`, `CONTRIBUTING.md` are created in Tasks 2/4 — that's fine, they land before merge.)

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: add top-level README"
```

---

### Task 2: `docs/configuration.md`

**Files:**
- Create: `docs/configuration.md`

**Interfaces:** Referenced by `README.md`. Facts sourced from `src/injector/key_map.rs`, `src/protocol.rs`, `src/config.rs`.

- [ ] **Step 1: Create `docs/configuration.md`** with this content (the key-name list is taken verbatim from `src/injector/key_map.rs` — do not add or rename entries):

````markdown
# Configuration reference

The GUI (Bindings / Profiles / Settings tabs) is the easy way to configure g13-driver and covers
everything most people need. This document is the reference for hand-editing the config files.

## Where config lives

Next to `g13-driver.exe`:

- `config.toml` — the **manifest**: global settings + which profile file each M-key uses.
- `profiles/*.toml` — one file per profile (a full binding set).

The app looks for `config.toml` **next to the exe first, then in the current directory**. Files are
**hot-reloaded** — save and changes apply immediately (no restart).

> A GUI **Save** rewrites the profile file and does **not** preserve comments or key order (known
> limitation). If you keep comments, edit by hand.

## The manifest (`config.toml`)

```toml
profiles_dir = "profiles"   # folder holding the profile files
m1 = "default.toml"         # profile loaded on M1
m2 = "game.toml"            # profile loaded on M2 (optional)
m3 = "media.toml"           # profile loaded on M3 (optional)

[autorepeat]                # optional; global auto-repeat timing
delay_ms = 400              # wait before repeating starts
interval_ms = 40            # gap between repeats (min 1)

[app]                       # managed by the GUI
start_active = false        # resume Active (true) or Dry-run (false) on launch
```

(A legacy `config.toml` that is itself a bare `[keys]` profile still works as a single M1 profile.)

## A profile file (`profiles/<name>.toml`)

```toml
[keys]
G1  = "ctrl+c"
G2  = "w"
BTN1 = "space"
STICK = "enter"

[repeat]        # optional: which bindings auto-repeat while held
G2 = true

[joystick]      # optional: map the stick to WASD
mode = "wasd"
deadzone = 30   # 0-127; distance from center before a direction fires
up = "w"
down = "s"
left = "a"
right = "d"
```

### Bindable inputs

`G1`–`G22`, `BTN1`, `BTN2` (the two thumb buttons), `STICK` (the joystick click). Names are
case-insensitive.

### Binding syntax

A binding is optional **modifiers** + one **key**: `modifier+modifier+key`. Modifiers: `ctrl`,
`shift`, `alt`, `win` (aliases `control`, `windows`). Examples: `a`, `ctrl+c`, `ctrl+shift+z`,
`win+d`. A modifier **alone** is allowed (e.g. `shift`). Bindings are **hold-means-hold** (held
while the G-key is held); multimedia keys are the tap-only exception.

### Valid key names

- **Letters:** `a`–`z`
- **Digits:** `0`–`9`
- **Function keys:** `f1`–`f24`
- **Modifiers:** `ctrl` (`control`), `shift`, `alt`, `win` (`windows`)
- **Editing / navigation:** `enter` (`return`), `esc` (`escape`), `space`, `tab`, `backspace`,
  `delete`, `insert`, `home`, `end`, `pageup`, `pagedown`, `up`, `down`, `left`, `right`
- **Locks / misc:** `capslock`, `printscreen`, `pause`, `numlock`, `scrolllock`
- **Media (tap-only):** `playpause`, `nexttrack` (`next`), `prevtrack` (`prev`), `mediastop`,
  `volup` (`volumeup`), `voldown` (`volumedown`), `mute`

An invalid key name is rejected (the GUI shows the binding as `bad`).

## Auto-repeat

Holding a bound key does **not** auto-repeat by default (a held key = one character). Tick **repeat**
on a binding (GUI) or add it to `[repeat]` to make it repeat while held, like a physical keyboard.
Timing is global in `[autorepeat]` (`delay_ms`, `interval_ms`). Joystick directions and modifier-only
bindings never repeat.

## Worked example

`profiles/game.toml` — WASD on the stick, jump on a thumb button, an auto-repeating attack:

```toml
[keys]
BTN1 = "space"     # jump
G1   = "e"         # interact
G2   = "r"         # reload / attack

[repeat]
G2 = true          # hold G2 to repeat "r"

[joystick]
mode = "wasd"
deadzone = 30
up = "w"
down = "s"
left = "a"
right = "d"
```
````

- [ ] **Step 2: Cross-check the key names against the code**

Run: `grep -oE '"[a-z0-9]+"' src/injector/key_map.rs | sort -u`
Confirm every key name you wrote in the "Valid key names" section appears in that output (letters/digits/f-keys are generated in loops — verify `a`, `z`, `0`, `9`, `f1`, `f24` build correctly per the loops at the top of `key_map.rs`). Fix any name that doesn't match the code.

- [ ] **Step 3: Commit**

```bash
git add docs/configuration.md
git commit -m "docs: add configuration reference"
```

---

### Task 3: SHA-pin Actions + Dependabot

**Files:**
- Modify: `.github/workflows/ci.yml`
- Modify: `.github/workflows/release.yml`
- Create: `.github/dependabot.yml`

**Interfaces:** none.

- [ ] **Step 1: Pin every `uses:` in `ci.yml` and `release.yml`**

Replace each floating tag with the pinned SHA + comment (from Global Constraints). In BOTH
`.github/workflows/ci.yml` and `.github/workflows/release.yml`, change:

- `uses: actions/checkout@v4` → `uses: actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5 # v4`
- `uses: msys2/setup-msys2@v2` → `uses: msys2/setup-msys2@66cd2cce69caa17b53920067426061ca1de3a884 # v2`
- `uses: Swatinem/rust-cache@v2` → `uses: Swatinem/rust-cache@e18b497796c12c097a38f9edb9d0641fb99eee32 # v2`
- `uses: actions/upload-artifact@v4` → `uses: actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02 # v4`
- `uses: actions/download-artifact@v4` → `uses: actions/download-artifact@d3f86a106a0bac45b974a628896c90dbdf5c8093 # v4`
- `uses: softprops/action-gh-release@v2` → `uses: softprops/action-gh-release@3bb12739c298aeb8a4eeaf626c5b8d85266b0e65 # v2`

(Not every action appears in both files — pin whichever `uses:` lines are present in each.)

- [ ] **Step 2: Create `.github/dependabot.yml`**

```yaml
version: 2
updates:
  - package-ecosystem: "github-actions"
    directory: "/"
    schedule:
      interval: "weekly"
    commit-message:
      prefix: "ci"
```

- [ ] **Step 3: Validate YAML**

Run:
```bash
python -c "import yaml; [yaml.safe_load(open(f)) for f in ['.github/workflows/ci.yml','.github/workflows/release.yml','.github/dependabot.yml']]; print('all valid')"
```
Expected: `all valid`.

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/ci.yml .github/workflows/release.yml .github/dependabot.yml
git commit -m "ci: pin Actions to SHAs + add Dependabot"
```

- [ ] **Step 5: Acceptance (controller-driven, real run)**

After merge to `main`, the controller confirms `ci.yml` still runs **green** with the pinned SHAs
(`gh run watch`). A red run here means a SHA typo or a pin to an incompatible commit — fix and
re-verify before proceeding to the visibility flip.

---

### Task 4: `CONTRIBUTING.md` + `SECURITY.md`

**Files:**
- Create: `CONTRIBUTING.md`
- Create: `SECURITY.md`

- [ ] **Step 1: Create `CONTRIBUTING.md`**

````markdown
# Contributing

Thanks for your interest in g13-driver! It's a small, Windows-only Rust project.

## Build & test

This machine uses the **GNU** Rust toolchain (not MSVC), because `rusb` compiles bundled libusb
from C via `cc` and needs a MinGW-w64 `gcc`:

```sh
rustup default stable-x86_64-pc-windows-gnu
# ensure a MinGW-w64 gcc is on PATH (e.g. MSYS2's mingw64, or Strawberry Perl's gcc)
cargo test            # unit tests
cargo build --release # -> target/release/g13-driver.exe
cargo run             # runs the GUI; --headless for the console driver
```

## How we work

Features go through a light spec → plan → build flow, tracked as design docs and milestones:

- Designs: `docs/superpowers/specs/`
- Plans: `docs/superpowers/plans/`
- Milestones (one file per unit of work): `milestones/`
- Agent/dev guidance and the non-obvious toolchain notes: `CLAUDE.md`

Pure-logic modules are built test-first (TDD); the USB and `SendInput` layers are the documented
manual-verify exception. Keep one focused commit per logical change with an imperative subject.

## Pull requests

- Run `cargo test` and `cargo build --release` before opening a PR.
- Describe what and why; link the milestone/design if relevant.
- CI must be green.

## License

By contributing, you agree your contributions are licensed under **GPL-3.0-or-later** (the project's
license) — inbound = outbound.
````

- [ ] **Step 2: Create `SECURITY.md`**

````markdown
# Security policy

## Supported versions

Only the latest [release](https://github.com/cavefish-dev/g13-driver/releases/latest) is supported.

## Reporting a vulnerability

Please report security issues **privately** using GitHub's
[**Report a vulnerability**](https://github.com/cavefish-dev/g13-driver/security/advisories/new)
(Security → Advisories), not a public issue.

This is a hobby project maintained on a best-effort basis — response times may vary, but security
reports are taken seriously.
````

- [ ] **Step 3: Commit**

```bash
git add CONTRIBUTING.md SECURITY.md
git commit -m "docs: add CONTRIBUTING and SECURITY"
```

---

### Task 5: Issue & PR templates

**Files:**
- Create: `.github/ISSUE_TEMPLATE/bug_report.md`
- Create: `.github/ISSUE_TEMPLATE/feature_request.md`
- Create: `.github/ISSUE_TEMPLATE/config.yml`
- Create: `.github/pull_request_template.md`

- [ ] **Step 1: Create `.github/ISSUE_TEMPLATE/bug_report.md`**

````markdown
---
name: Bug report
about: Something isn't working
labels: bug
---

**What happened**
A clear description of the bug and what you expected instead.

**To reproduce**
Steps to trigger it.

**Environment**
- Windows version:
- g13-driver version (title bar / release):
- Did you run the Zadig/WinUSB setup? (yes/no)
- Mode when it happened (Dry-run / Active):

**Logs**
Run from a terminal with `RUST_LOG=debug g13-driver.exe --headless` and paste relevant output.
````

- [ ] **Step 2: Create `.github/ISSUE_TEMPLATE/feature_request.md`**

````markdown
---
name: Feature request
about: Suggest an idea
labels: enhancement
---

**The problem / use case**
What are you trying to do?

**Proposed solution**
What you'd like to see.

**Alternatives / notes**
Anything else (see the Roadmap in the README for what's already planned).
````

- [ ] **Step 3: Create `.github/ISSUE_TEMPLATE/config.yml`**

```yaml
blank_issues_enabled: false
contact_links:
  - name: Setup & configuration help
    url: https://github.com/cavefish-dev/g13-driver#quick-start
    about: The README and docs/configuration.md cover install and config.
```

- [ ] **Step 4: Create `.github/pull_request_template.md`**

````markdown
## What & why

<!-- What does this change, and why? Link a milestone/design if relevant. -->

## Checklist

- [ ] `cargo test` passes
- [ ] `cargo build --release` succeeds
- [ ] Followed the project conventions (see CONTRIBUTING.md)
````

- [ ] **Step 5: Commit**

```bash
git add .github/ISSUE_TEMPLATE .github/pull_request_template.md
git commit -m "docs: add issue and PR templates"
```

---

### Task 6: Repo metadata + go public (controller, gated on user)

**Files:** none (GitHub settings via `gh`).

This task is **controller-driven** and the visibility flip happens **only on the user's explicit
go**, after Tasks 1–5 are merged to `main` and CI is green (Task 3 acceptance).

- [ ] **Step 1: Set description + topics**

```bash
gh repo edit cavefish-dev/g13-driver \
  --description "Open-source replacement driver for the Logitech G13 keypad (Windows) — remap keys, joystick→WASD, profiles, tray app" \
  --add-topic logitech-g13 --add-topic g13 --add-topic keypad --add-topic rust \
  --add-topic windows --add-topic driver --add-topic hid --add-topic libusb \
  --add-topic gaming --add-topic egui
```

- [ ] **Step 2: Flip to public (ONLY on explicit user go)**

```bash
gh repo edit cavefish-dev/g13-driver --visibility public --accept-visibility-change-consequences
```

- [ ] **Step 3: Verify**

```bash
gh repo view cavefish-dev/g13-driver --json visibility,description,repositoryTopics
```
Expected: `visibility: PUBLIC`, the description set, topics listed.

- [ ] **Step 4: Milestone**

Create `milestones/finished/public-release-readiness.md`:
```markdown
# Public-release readiness

- **Status:** finished
- **Date:** <fill in on completion>

## Outcome
Repo prepared for and flipped to public. Spec:
`docs/superpowers/specs/2026-07-13-public-release-readiness-design.md`.
- README.md (GUI-first, beginner-friendly) + docs/configuration.md (TOML reference, verified
  against key_map.rs/protocol.rs/config.rs).
- SHA-pinned all GitHub Actions + Dependabot (github-actions, weekly). CI re-verified green.
- CONTRIBUTING.md, SECURITY.md (private advisories), issue/PR templates, repo description + topics.
- Repo flipped public.

## Follow-ups
- Screenshots in the README; code-signing the exe; CODE_OF_CONDUCT.md; cargo Dependabot ecosystem.
- Next: sub-project #3 (in-app auto-update from GitHub Releases).
```

```bash
git add milestones/finished/public-release-readiness.md
git commit -m "docs: public-release-readiness milestone"
```

---

## Notes for the executor

- Tasks 1, 2, 4, 5 are content files (transcribe verbatim; verify links/key-names). Task 3 pins
  Actions — the acceptance is a green CI run post-merge (controller via `gh`). Task 6 is
  controller-driven and the **visibility flip requires explicit user confirmation**.
- No Rust code changes; `cargo test` stays at 107 passing.
- After all tasks: a final whole-branch review (docs review — accuracy, links, tone, no secrets),
  then `superpowers:finishing-a-development-branch` (merge), then Task 3 CI re-verify, then Task 6
  metadata + the gated public flip.
