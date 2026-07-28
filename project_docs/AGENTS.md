# Project Documentation Instructions

These instructions apply under `project_docs/` in addition to the repository
root rules.

## Routing and authority

Public user and developer documentation belongs under `mdbook/src/`. Keep
`project_docs/` limited to agent context, test/evidence metadata, decision
provenance, internal work tracking, and archives. Do not add a public guide or
competing feature summary here.

Read `README.md` first. It defines authority order, task routing, lifecycle,
and context policy. `DOCUMENT_REGISTRY.json` is the machine-readable inventory.
Do not load every project document for a localized task.

- Current code, manifests, and tests outrank narrative documentation.
- `CODEX_ACTIVE_WORK.md` contains only the live handoff and at most one current
  state.
- Accepted architecture decisions and project context are durable direction,
  not substitutes for current code.
- Historical files are provenance. Keep them excluded from default context and
  open only the relevant section for a specific question.
- Preserve dates, hashes, feature sets, artifact identities, and limitations
  when moving evidence.

Every added, moved, or removed Markdown document under `project_docs/` must be
reconciled with `DOCUMENT_REGISTRY.json`. Give high-context entry points a
reviewed byte budget.

## Test documentation

Start at `testing/README.md`. Keep test definitions separate from dated run
evidence:

- catalog entries define selection, prerequisites, procedure, stop conditions,
  and required evidence;
- new run results belong in bounded records under `testing/evidence/`; raw
  logs and samples remain referenced artifacts rather than inline content;
- pre-rollout results, hashes, measurements, and operator observations remain
  in retained historical evidence;
- active board-specific hardware procedures belong under `testing/targets/`
  and remain bounded;
- `active` means selectable after prerequisites, not passed or flight-cleared;
- powered and flight execution remains with the user.

Reconcile a procedure with current code and `CODEX_ACTIVE_WORK.md` before
promoting it from `needs-review`.

## Editing rules

- Update existing authoritative text instead of adding a competing summary.
- Route to detailed evidence rather than copying long historical sections.
- Archive superseded state instead of appending another live-state block.
- Do not weaken a safety limitation to make documentation appear complete.
- Use certification-aligned language and state unverified claims explicitly.

Run after documentation changes:

```text
python tools/check_repository_context.py
python -m unittest tools.tests.test_repository_context -v
mdbook build mdbook
```
