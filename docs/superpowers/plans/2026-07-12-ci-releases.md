# CI & version.txt-driven releases Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** GitHub Actions CI that builds+tests on push/PR and, on a `version.txt` bump, publishes a GitHub Release (zip bundle + SHA256) — the artifacts sub-project #3 will auto-update from.

**Architecture:** `version.txt` (bare semver) is the single source of truth, stamped into the binary via `build.rs` (`env!("G13_VERSION")`), with a drift guard requiring `Cargo.toml` to match. Two workflows (`ci.yml`, `release.yml`) share one extensible build matrix (one Windows-GNU row today). The release flow is `prepare` (guard + idempotency) → `build` (matrix, package) → `publish` (one full GitHub Release with auto-generated notes).

**Tech Stack:** Rust (GNU toolchain), `build.rs`, GitHub Actions (windows-latest), MinGW-w64 gcc (for `rusb`'s libusb), `Swatinem/rust-cache`, `softprops/action-gh-release`.

Full design: `docs/superpowers/specs/2026-07-12-ci-releases-design.md`.

## Global Constraints

- Build with the **GNU** toolchain (`x86_64-pc-windows-gnu`); NOT MSVC. `rusb` builds bundled libusb from C via `cc`, needing a MinGW-w64 `gcc` on PATH. Local dev PATH prefix if needed: `export PATH="$HOME/.cargo/bin:/c/Strawberry/c/bin:$PATH"`.
- **Binary** crate: `cargo test` (NOT `cargo test --lib`).
- **`version.txt`** holds a bare semver (e.g. `0.1.0`), single trailing newline, no `v` prefix. It equals `Cargo.toml`'s `[package] version`; the drift guard fails the build otherwise.
- **The crate is Windows-only** (`src/main.rs:3-4` `compile_error!` on non-Windows) — the matrix has one Windows row today; non-Windows rows need that relaxed (out of scope).
- **Release naming:** tag `v<VERSION>`, asset `g13-driver-v<VERSION>-<name>.zip` (+ `.zip.sha256`), where `<name>` is the matrix row's name (`windows-x64`).
- **No new secrets** — only the built-in `GITHUB_TOKEN`; `release.yml` uses `permissions: contents: write`.
- Actions pinned to a version tag; releases are **full/latest** (not draft) with **`generate_release_notes: true`**.
- Workflows are **manual-verify** (the CI-iterate-on-real-run exception): acceptance is a green `ci.yml` run and a real Release from a `version.txt` bump. The MinGW step is the most likely thing to need tweaking on the first real run.
- One focused commit per task; imperative subject; end each commit message with:
  `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`

---

### Task 1: `version.txt` + `build.rs` version stamping + startup log

**Files:**
- Create: `version.txt`
- Create: `build.rs`
- Modify: `src/main.rs` (startup log + a `#[cfg(test)]` version test)

**Interfaces:**
- Produces: compile-time env var `G13_VERSION` (read via `env!("G13_VERSION")`), equal to the trimmed contents of `version.txt`.

- [ ] **Step 1: Create `version.txt`**

Create `version.txt` at the repo root with exactly (matching the current `Cargo.toml` version `0.1.0`), one trailing newline:
```
0.1.0
```

- [ ] **Step 2: Write the failing test**

Add to `src/main.rs` (at the end of the file):
```rust
#[cfg(test)]
mod version_tests {
    #[test]
    fn binary_version_matches_version_txt() {
        let file = std::fs::read_to_string("version.txt").expect("version.txt at crate root");
        assert_eq!(env!("G13_VERSION"), file.trim());
        assert!(!env!("G13_VERSION").is_empty());
    }
}
```

- [ ] **Step 3: Run it to verify it fails**

Run: `cargo test binary_version_matches_version_txt`
Expected: FAIL — a **compile error** `environment variable 'G13_VERSION' not defined` (because there is no `build.rs` emitting it yet). That is the RED state.

- [ ] **Step 4: Add `build.rs` and the startup log**

Create `build.rs` at the repo root:
```rust
use std::fs;

fn main() {
    let version = fs::read_to_string("version.txt")
        .expect("version.txt not found at crate root")
        .trim()
        .to_string();
    assert!(!version.is_empty(), "version.txt is empty");
    println!("cargo:rustc-env=G13_VERSION={version}");
    println!("cargo:rerun-if-changed=version.txt");
}
```

In `src/main.rs`, add the startup version log immediately after `env_logger::init();`:
```rust
    env_logger::init();
    log::info!("g13-driver v{}", env!("G13_VERSION"));
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test binary_version_matches_version_txt` then `cargo test`
Expected: PASS (the new test) and all existing tests still pass.

- [ ] **Step 6: Commit**

```bash
git add version.txt build.rs src/main.rs
git commit -m "feat: stamp version.txt into the binary as G13_VERSION"
```

---

### Task 2: Version drift-guard script + `rust-toolchain.toml`

**Files:**
- Create: `ci/check-version.sh`
- Create: `rust-toolchain.toml`

**Interfaces:**
- Produces: `ci/check-version.sh` — exits 0 if `Cargo.toml` `[package] version` == `version.txt`, else prints the mismatch and exits 1. Runnable locally and from CI (`shell: bash`).

- [ ] **Step 1: Create the drift-guard script**

Create `ci/check-version.sh`:
```bash
#!/usr/bin/env bash
# Fail if Cargo.toml's [package] version does not match version.txt.
set -euo pipefail

cargo_ver="$(grep -m1 '^version[[:space:]]*=' Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/')"
file_ver="$(tr -d '[:space:]' < version.txt)"

if [ "$cargo_ver" != "$file_ver" ]; then
  echo "version mismatch: Cargo.toml=$cargo_ver version.txt=$file_ver" >&2
  echo "bump BOTH to the same value." >&2
  exit 1
fi
echo "version ok: $file_ver"
```
(`^version[[:space:]]*=` matches only the top-level `[package]` `version = "..."` line — dependency versions like `serde = { version = ... }` do not start a line with `version`.)

- [ ] **Step 2: Run it to verify it passes on the matching repo**

Run (from repo root): `bash ci/check-version.sh`
Expected: prints `version ok: 0.1.0` and exits 0 (both files are `0.1.0`).

- [ ] **Step 3: Verify it fails on a mismatch**

Run:
```bash
printf '9.9.9\n' > /tmp/vtest && cp version.txt /tmp/vbak && cp /tmp/vtest version.txt
bash ci/check-version.sh; echo "exit=$?"
cp /tmp/vbak version.txt
```
Expected: prints `version mismatch: Cargo.toml=0.1.0 version.txt=9.9.9` and `exit=1`, then restores `version.txt`.

- [ ] **Step 4: Create `rust-toolchain.toml`**

Create `rust-toolchain.toml` at the repo root:
```toml
[toolchain]
channel = "stable"
targets = ["x86_64-pc-windows-gnu"]
profile = "minimal"
```

- [ ] **Step 5: Confirm the build still works with the pinned file**

Run: `cargo build`
Expected: builds clean (the local default host is already `x86_64-pc-windows-gnu`; this file just pins it for anyone cloning).

- [ ] **Step 6: Commit**

```bash
git add ci/check-version.sh rust-toolchain.toml
git commit -m "chore: version drift-guard script + pinned rust-toolchain"
```

---

### Task 3: `ci.yml` — build + test on push/PR

**Files:**
- Create: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: `ci/check-version.sh` (Task 2), the GNU toolchain, MinGW gcc.

This task is **manual-verify** — acceptance is a green run on the real runner (the controller triggers it via a PR and watches with `gh`). The MinGW/toolchain steps are the iterate-on-real-run part.

- [ ] **Step 1: Create the workflow**

Create `.github/workflows/ci.yml`:
```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  build-test:
    strategy:
      fail-fast: false
      matrix:
        include:
          - { name: windows-x64, os: windows-latest, toolchain: stable-x86_64-pc-windows-gnu }
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4

      - name: Version drift guard
        shell: bash
        run: bash ci/check-version.sh

      - name: MinGW-w64 gcc (for rusb/libusb)
        uses: msys2/setup-msys2@v2
        with:
          msystem: MINGW64
          install: mingw-w64-x86_64-gcc
          update: false
      - name: Put MinGW on PATH
        shell: bash
        run: echo "$(cygpath -w /mingw64/bin)" >> "$GITHUB_PATH"

      - name: Install GNU Rust toolchain
        shell: bash
        run: |
          rustup toolchain install ${{ matrix.toolchain }} --profile minimal
          rustup default ${{ matrix.toolchain }}
          rustc -vV
          gcc --version

      - name: Cache cargo + target
        uses: Swatinem/rust-cache@v2

      - name: Test
        shell: bash
        run: cargo test

      - name: Build (release)
        shell: bash
        run: cargo build --release
```

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: build + test on push/PR (windows-gnu matrix)"
```

- [ ] **Step 3: Acceptance (controller-driven, real run)**

The controller pushes the branch and opens a PR to `main`, then watches the `CI` workflow with `gh run watch` / `gh pr checks`. Iterate on the MinGW/toolchain steps until the run is green (build + tests pass on `windows-latest`). Do NOT consider the task complete until a real run passes.

---

### Task 4: `release.yml` — publish a Release on `version.txt` bump

**Files:**
- Create: `.github/workflows/release.yml`
- Create: `packaging/README.txt`

**Interfaces:**
- Consumes: `ci/check-version.sh`, `version.txt`, the built binary at `target/release/g13-driver.exe`, `config.toml`, `profiles/`, `LICENSE`.

Also **manual-verify** — acceptance is a real Release created from a `version.txt` bump.

- [ ] **Step 1: Create the bundle README**

Create `packaging/README.txt`:
```
g13-driver — open-source Logitech G13 driver

First run:
  1. Plug in the G13. Run Zadig once to install the WinUSB driver on the G13
     (see the project's docs/zadig-setup.md). This is a one-time step.
  2. Keep g13-driver.exe, config.toml and the profiles/ folder together in one
     folder — the app reads config.toml from next to the exe.
  3. Run g13-driver.exe. Close/minimize hides it to the tray; Quit from the tray
     to exit. Optional: enable "Launch at login" in Settings.

License: GPL-3.0-or-later (see LICENSE).
Project: https://github.com/cavefish-dev/g13-driver
```

- [ ] **Step 2: Create the release workflow**

Create `.github/workflows/release.yml`:
```yaml
name: Release

on:
  push:
    branches: [main]
    paths: [version.txt]

permissions:
  contents: write

concurrency:
  group: release-${{ github.ref }}
  cancel-in-progress: false

jobs:
  prepare:
    runs-on: ubuntu-latest
    outputs:
      version: ${{ steps.v.outputs.version }}
      tag: ${{ steps.v.outputs.tag }}
      should_release: ${{ steps.v.outputs.should_release }}
    steps:
      - uses: actions/checkout@v4
      - name: Read version, guard, check existing release
        id: v
        env:
          GH_TOKEN: ${{ github.token }}
        shell: bash
        run: |
          bash ci/check-version.sh
          VERSION="$(tr -d '[:space:]' < version.txt)"
          TAG="v$VERSION"
          echo "version=$VERSION" >> "$GITHUB_OUTPUT"
          echo "tag=$TAG" >> "$GITHUB_OUTPUT"
          if gh release view "$TAG" >/dev/null 2>&1; then
            echo "release $TAG already exists — skipping"
            echo "should_release=false" >> "$GITHUB_OUTPUT"
          else
            echo "should_release=true" >> "$GITHUB_OUTPUT"
          fi

  build:
    needs: prepare
    if: needs.prepare.outputs.should_release == 'true'
    strategy:
      fail-fast: false
      matrix:
        include:
          - { name: windows-x64, os: windows-latest, toolchain: stable-x86_64-pc-windows-gnu }
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - name: MinGW-w64 gcc (for rusb/libusb)
        uses: msys2/setup-msys2@v2
        with:
          msystem: MINGW64
          install: mingw-w64-x86_64-gcc
          update: false
      - name: Put MinGW on PATH
        shell: bash
        run: echo "$(cygpath -w /mingw64/bin)" >> "$GITHUB_PATH"
      - name: Install GNU Rust toolchain
        shell: bash
        run: |
          rustup toolchain install ${{ matrix.toolchain }} --profile minimal
          rustup default ${{ matrix.toolchain }}
      - name: Cache cargo + target
        uses: Swatinem/rust-cache@v2
      - name: Build (release)
        shell: bash
        run: cargo build --release
      - name: Package bundle + SHA256
        shell: bash
        env:
          VERSION: ${{ needs.prepare.outputs.version }}
          NAME: ${{ matrix.name }}
        run: |
          STAGE="g13-driver-v${VERSION}-${NAME}"
          mkdir "$STAGE"
          cp target/release/g13-driver.exe "$STAGE"/
          cp config.toml "$STAGE"/
          cp -r profiles "$STAGE"/
          cp LICENSE "$STAGE"/
          cp packaging/README.txt "$STAGE"/
          7z a "${STAGE}.zip" "$STAGE" >/dev/null
          sha256sum "${STAGE}.zip" > "${STAGE}.zip.sha256"
          ls -l "${STAGE}.zip" "${STAGE}.zip.sha256"
      - name: Upload build artifacts
        uses: actions/upload-artifact@v4
        with:
          name: ${{ matrix.name }}
          path: |
            g13-driver-v*-*.zip
            g13-driver-v*-*.zip.sha256

  publish:
    needs: [prepare, build]
    runs-on: ubuntu-latest
    steps:
      - name: Download all artifacts
        uses: actions/download-artifact@v4
        with:
          path: dist
          merge-multiple: true
      - name: Create GitHub Release
        uses: softprops/action-gh-release@v2
        with:
          tag_name: ${{ needs.prepare.outputs.tag }}
          name: ${{ needs.prepare.outputs.tag }}
          generate_release_notes: true
          files: |
            dist/*.zip
            dist/*.zip.sha256
```

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/release.yml packaging/README.txt
git commit -m "ci: publish a GitHub Release on version.txt bump"
```

- [ ] **Step 4: Acceptance (controller-driven, real run)**

After `ci.yml` (Task 3) is green and merged to `main`, the controller cuts a real release: bump `version.txt` **and** `Cargo.toml` to the next version (e.g. `0.2.0`) in one commit on `main`, push, and watch the `Release` workflow with `gh run watch`. Confirm: a `v0.2.0` Release is created, **full/latest**, with auto-generated notes and both `g13-driver-v0.2.0-windows-x64.zip` and its `.sha256` attached; download the zip and verify it contains the exe + `config.toml` + `profiles/` + `LICENSE` + `README.txt`, and that the sha256 matches. Re-running (or a no-op `version.txt` touch of the same value) must NOT create a duplicate release.

---

### Task 5: Docs + milestone

**Files:**
- Modify: `CLAUDE.md` (release process)
- Create: `milestones/finished/ci-releases.md`

- [ ] **Step 1: Document the release process in `CLAUDE.md`**

Add a short "Releases" subsection under the Build & test area of `CLAUDE.md`:
```markdown
## Releases (CI)

- CI (`.github/workflows/ci.yml`) builds + tests every push/PR to `main` on `windows-latest`
  with the GNU toolchain + MinGW gcc.
- `version.txt` (repo root, bare semver) is the single source of truth for the release version;
  `build.rs` stamps it into the binary as `env!("G13_VERSION")`. It MUST equal `Cargo.toml`'s
  `[package] version` — `ci/check-version.sh` fails the build otherwise.
- To cut a release: bump BOTH `version.txt` and `Cargo.toml` to the same new semver in one commit
  on `main`. `.github/workflows/release.yml` then tags `vX.Y.Z` and publishes a GitHub Release with
  the zip bundle (`g13-driver-vX.Y.Z-windows-x64.zip`) + `.sha256`. Re-runs are idempotent.
```

- [ ] **Step 2: Write the milestone**

Create `milestones/finished/ci-releases.md`:
```markdown
# CI + version.txt-driven releases

- **Status:** finished
- **Date:** <fill in on completion>

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
```

- [ ] **Step 3: Commit**

```bash
git add CLAUDE.md milestones/finished/ci-releases.md
git commit -m "docs: release process + ci-releases milestone"
```

---

## Notes for the executor

- Tasks 1-2 are ordinary TDD/local Rust+shell (implementer + review). Tasks 3-4 are **manual-verify**: the implementer writes the YAML; the **controller** runs the real GitHub Actions verification via `gh` (push a branch + PR for `ci.yml`; a `version.txt`+`Cargo.toml` bump on `main` for `release.yml`) and iterates on the MinGW/toolchain steps until green. Expect 1-3 push/watch cycles on the MinGW step.
- After all tasks: final whole-branch review (most capable model), then `superpowers:finishing-a-development-branch`.
- The `release.yml` acceptance actually cuts `v0.2.0` — that is the first real release and is expected; it also bumps the working version to `0.2.0`.
