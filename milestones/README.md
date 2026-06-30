# Milestones

Lightweight task tracking for this project. One markdown file per milestone, filed by
lifecycle state. Move the file between folders as its state changes.

## States

| Folder | Meaning |
|--------|---------|
| `open/` | Defined but not started. The backlog. |
| `ongoing/` | Actively being worked on. Keep its checklist current. |
| `finished/` | Completed. Kept for reference / changelog. |
| `archived/` | Dropped, deferred indefinitely, or superseded. Keep a note on *why*. |

## Workflow

1. **Start work:** move a file from `open/` to `ongoing/`, set `Status: ongoing`.
2. **As you progress:** tick checklist items in the file; add notes/decisions inline.
3. **Done:** move to `finished/`, set `Status: finished`, record the date and outcome.
4. **Abandon/defer:** move to `archived/`, set `Status: archived`, write why.

Keep only one or two milestones in `ongoing/` at a time. Prefer small, shippable milestones.

## File format

Name files `NN-short-slug.md` (or `vX.Y-slug.md` for release phases). Each file has:

```markdown
# <title>

- **Status:** open | ongoing | finished | archived
- **Target:** <version or "next"> 
- **Updated:** YYYY-MM-DD

## Goal
One or two sentences.

## Tasks
- [ ] ...

## Acceptance
What "done" means / how to verify.

## Notes
Decisions, blockers, links.
```
