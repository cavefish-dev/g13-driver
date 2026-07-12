# CI & version.txt-driven releases — design

- **Status:** approved (design)
- **Date:** 2026-07-12
- **Scope:** GitHub Actions CI that (a) builds + tests the Windows release binary on every push/PR, and
  (b) publishes a GitHub Release (zip bundle + SHA256) when `version.txt` is bumped. The release
  artifacts are what sub-project #3 (auto-update) will pull. MVP sub-project #2 of 3.

## Motivation

The driver is about to go public and needs reproducible builds and a release channel. A single
`version.txt` (semver) is the source of truth: bumping it cuts a release; the running binary
self-reports that same version so the future auto-updater can compare against the latest Release.

## Constraints (from the project)

- **Binary crate, GNU toolchain only.** `rusb` builds bundled libusb from C via `cc`, so the build
  needs Rust's `x86_64-pc-windows-gnu` target **and** a real MinGW-w64 `gcc` on PATH (locally this is
  Strawberry Perl's gcc). No MSVC.
- The crate is **Windows-only** today: `src/main.rs` has `#![cfg_attr(not(windows), … compile_error!)]`,
  so a non-Windows build fails. Multi-arch/cross rows are prepared for but not enabled here.
- The app needs `config.toml` + `profiles/` next to the exe, so a release ships a **bundle**, not a
  bare exe.
- Repo `cavefish-dev/g13-driver`, GPL-3.0-or-later. Only the built-in `GITHUB_TOKEN` (no new secrets).

## Architecture

Two workflows under `.github/workflows/` plus a pinned toolchain, both driven by one shared,
extensible build **matrix**.

- **`ci.yml`** — on `push` and `pull_request` to `main`: per matrix row, `cargo test` +
  `cargo build --release`. Fast feedback.
- **`release.yml`** — on `push` to `main` filtered to `paths: [version.txt]`: build each row's
  artifact and publish one GitHub Release.
- **`rust-toolchain.toml`** (repo root) pinning `channel = "stable"` and
  `targets = ["x86_64-pc-windows-gnu"]` so local and CI resolve the same toolchain.

### The matrix (one row now, extensible)

A single `strategy.matrix.include` list consumed by both workflows. Today:
```yaml
include:
  - { name: windows-x64, os: windows-latest, target: x86_64-pc-windows-gnu, cross: false }
```
Each row carries the runner `os`, Rust `target` triple, a `cross` flag (future cross-compiled rows),
and an artifact `name`. Steps branch on `os`/`cross`. Adding a target later is one entry — but a
non-Windows row won't build until the top-level `compile_error!` is relaxed to gate only the
Windows-specific `main`/injector (a later, out-of-scope code change).

## Version stamping & drift guard

- `version.txt` (repo root) holds the bare semver, e.g. `0.2.0` (no `v`), single trailing newline.
- **Stamped into the binary:** a `build.rs` reads `version.txt` and emits
  `cargo:rustc-env=G13_VERSION=<contents-trimmed>` plus `cargo:rerun-if-changed=version.txt`. The app
  reads its version via `env!("G13_VERSION")`. So the binary self-reports exactly `version.txt` (what
  #3 compares against the latest Release). The version is surfaced in a startup log line.
- **Drift guard:** a check that `Cargo.toml`'s `[package] version` == `version.txt`; it fails the
  build if they differ. Runs in `ci.yml` and again at the start of `release.yml`. Cutting a release
  therefore means bumping **both** `version.txt` and `Cargo.toml` to the same value in one commit.
  Implemented as a small script (e.g. `ci/check-version.sh` or an inline shell step) so it is also
  runnable locally.

## Release workflow flow (`release.yml`)

Three jobs so the matrix builds in parallel but the release is created once:

1. **`prepare`** — read `version.txt` → `VERSION`; run the drift guard; compute `TAG=v$VERSION`;
   check whether tag/release `TAG` already exists (via `gh release view` / the API). Outputs
   `version`, `tag`, `should_release` (false if the release already exists → idempotent skip).
2. **`build`** (`needs: prepare`, `if: should_release`, `strategy.matrix`) — per row: build the
   release binary (version stamped in), package the bundle + SHA256 (see Packaging), upload both as
   job artifacts.
3. **`publish`** (`needs: build`) — download all row artifacts; create the GitHub Release for `TAG`
   (creating the tag on the release commit), **full/latest (not draft)**, with
   **`generate_release_notes: true`** (GitHub auto-generated notes), and attach every zip + `.sha256`.
   `permissions: contents: write`; `GITHUB_TOKEN` only. A `concurrency` group on `release.yml` keyed
   by the version prevents two quick pushes from double-releasing.

## Build environment (per matrix row)

Shared steps, branching on `os`/`target`, all **pinned** (actions to a version/SHA):

1. `actions/checkout`.
2. Rust toolchain via `dtolnay/rust-toolchain` for the row's `target` (aligns with
   `rust-toolchain.toml`).
3. **MinGW gcc (gnu rows):** install MinGW-w64 gcc and put it on PATH so `cc` can build libusb — via
   `msys2/setup-msys2` installing `mingw-w64-x86_64-gcc` (adds `mingw64/bin`). Non-gnu rows would
   skip/replace this.
4. **Cache:** `Swatinem/rust-cache@v2`, keyed per matrix row (os/target + `Cargo.lock`), caching
   `~/.cargo` + `target/` so the compiled libusb and deps restore on later runs.
5. Build/test: `ci.yml` → `cargo test` then `cargo build --release`; `release.yml` → `cargo build
   --release` (tests already gate via `ci.yml`).

**Honest caveat:** the exact MinGW step is the most likely thing to need a tweak on the first real
run (runner image, PATH, gcc/rustc-mingw compatibility). Acceptance for the workflow pieces is a
green run on the real runner — they can't be verified locally.

## Packaging & artifacts (per matrix row, in `build`)

- Stage: `g13-driver.exe` + `config.toml` + `profiles/` (default/game/media.toml) + `LICENSE` + a
  short `README.txt` (first-run note: run Zadig once per `docs/zadig-setup.md`; keep the files
  together).
- Zip → `g13-driver-v<VERSION>-<name>.zip` (e.g. `…-windows-x64.zip`; `name` from the matrix row).
- SHA256 → `g13-driver-v<VERSION>-<name>.zip.sha256`.
- Upload both via `actions/upload-artifact`; `publish` downloads all and attaches them to the Release.

## Testing

- **Unit (in the existing suite):** `build.rs` stamping — a test asserting `env!("G13_VERSION")` is
  non-empty and equals the trimmed contents of `version.txt` (read at test time). The drift-guard
  comparison logic is a small script, eyeballed + exercised by CI.
- **Manual-verify (documented CI-iterate exception):** the workflows themselves. Acceptance:
  `ci.yml` goes green on a PR; a `version.txt` (+`Cargo.toml`) bump on `main` produces a real
  Release named `vX.Y.Z` with the zip + `.sha256` attached; a re-run / non-bump push does not create
  a duplicate.

## Error handling / safety

- Drift guard and idempotency guard fail fast / skip — no partial or duplicate releases.
- `permissions` scoped to `release.yml` (`contents: write`); no new secrets.
- `concurrency` guard on `release.yml` keyed by version.
- Actions and toolchain pinned for reproducibility.

## Out of scope (follow-ups)
- **Code signing** the exe (needs a certificate) — until then Windows SmartScreen warns on first run.
- Enabling non-Windows / cross-compiled matrix rows (needs the `compile_error!` relaxed so pure-logic
  modules build cross-platform).
- `%APPDATA%` config location + downloading profiles from the public repo (distribution follow-up).
- Sub-project #3: auto-update pulling these Release artifacts (compares `env!("G13_VERSION")` vs the
  latest Release, verifies SHA256, applies without clobbering user-edited config/profiles).
