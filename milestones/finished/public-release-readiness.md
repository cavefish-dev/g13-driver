# Public-release readiness

- **Status:** finished (repo prepared + cleaned; the public visibility flip is the owner's to do)
- **Date:** 2026-07-13

## Outcome
The repo is prepared for public release. Spec:
`docs/superpowers/specs/2026-07-13-public-release-readiness-design.md`; plan:
`docs/superpowers/plans/2026-07-13-public-release-readiness.md`.

- **README.md** (beginner-friendly, GUI-first) + **docs/configuration.md** (TOML/key reference,
  verified against `src/injector/key_map.rs`/`protocol.rs`/`config.rs`).
- **CONTRIBUTING.md**, **SECURITY.md** (private advisories), **issue/PR templates**, repo
  **description + 10 topics**.
- **Supply-chain hardening:** all GitHub Actions SHA-pinned + **Dependabot** (github-actions,
  weekly). `contents: write` scoped to the `publish` job only. CI re-verified green with the pins.
- **CI trigger scoped** to source/build changes (paths filter) so docs-only commits don't run the
  full build. `release.yml` gained a `workflow_dispatch` manual trigger.

## Sensitive-info audit + email scrub
- Audited full git history + tracked files: no tokens/keys/secrets, no username/machine-path leaks,
  `.superpowers/` scratch never committed, no binaries tracked.
- The one finding: the commit-author email was a personal Gmail. Switched local
  `git config user.email` to the GitHub **noreply** address, rewrote all history to it
  (`filter-branch`), and — because GitHub retains `refs/pull/*` from PRs — **deleted and recreated
  the repo** to eliminate every trace. Verified: **zero gmail commits** across all refs (branches,
  tags, PR refs); only the owner's noreply + Dependabot's bot noreply remain.
- Backups saved before recreation: `g13-driver-backup-2026-07-13.bundle` (full history) and
  `g13-driver-backup-2026-07-13.zip` (files), in the parent folder.
- After recreate: pushed clean history, republished the **v0.1.0** release (rebuilt bundle +
  SHA256) via `workflow_dispatch`, re-set description/topics. Repo remains **private**.

## Follow-ups
- The owner flips visibility to public when ready (`gh repo edit --visibility public`).
- Dependabot re-opened PRs bumping the pinned Actions to newer majors (checkout v7, upload v7,
  download v8, gh-release v3) — review/merge or dismiss at leisure (major bumps, test first).
- Deferred: README screenshots, code-signing the exe, `CODE_OF_CONDUCT.md`, cargo Dependabot.
- Next: sub-project #3 (in-app auto-update from GitHub Releases).
