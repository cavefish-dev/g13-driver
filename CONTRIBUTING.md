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
