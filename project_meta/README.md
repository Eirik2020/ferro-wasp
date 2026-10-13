# Internal Project Metadata

Public user and developer documentation lives in the
[mdBook](../mdbook/src/SUMMARY.md). This directory is not a documentation entry
point for users or contributors.

`project_meta` retains information needed for agent routing, engineering
continuity, machine-enforced test selection, evidence provenance, and internal
work tracking. `DOCUMENT_REGISTRY.json` is the machine-readable inventory. Its
lifecycle and context fields are enforced by
`tools/check_repository_context.py`.

Do not add public guides, feature summaries, setup instructions, or roadmap
material here. Update the relevant mdBook chapter instead.

## Authority Order

The root `AGENTS.md` defines the authority order. Below it and
[the governing requirements](governing-requirements.md), current code,
manifests, and tests outrank every narrative document here.

Historical evidence can remain technically useful without describing current
defaults.

Immutable archives may retain the former `project_docs` path when it was part
of the recorded source tree. Those strings are historical provenance, not live
routing; current references must use `project_meta`.

## Agent Task Routing

| Task | Read |
|---|---|
| Software, target, bench, preflight, or flight testing | `testing/README.md`, then only the catalog-selected procedures |
| Localized reusable-crate work | Selected crate code and tests; use `CODEX_PROJECT_CONTEXT.md` only when architecture is affected |
| Flight app, motor output, logging, or bench workflow | `CODEX_ACTIVE_WORK.md`, selected app code, and the catalog-selected procedure |
| Board pins, timers, DMA, orientation, or hardware policy | `ARCHITECTURE_DECISIONS.md`, the relevant ADRs, selected app support, and current target evidence |
| Cross-repository movement or ownership | `CROSS_REPO_SYNC.md` |
| Publication work | `PUBLICATION_CHECKLIST.md` and the mdBook |
| Historical provenance | The relevant file under `archive/` or another registry entry marked `historical` |

Files marked `targeted` should be read only when the task requires them. Files
marked `exclude` are never default context and must be opened only for a
specific provenance question.

## Live and Historical Work

`CODEX_ACTIVE_WORK.md` contains only the live handoff. Completed checkpoints
and superseded state belong under `archive/`; do not append another dated
current-state section to the live file.

Moving material to the archive does not weaken its evidence status. Preserve
artifact identities, hashes, dates, feature sets, and limitations when
archiving. Use `testing/EVIDENCE_INDEX.md` to locate retained evidence without
loading complete immutable baselines.

## Context Guard

Run:

```text
python tools/check_repository_context.py
```

The check verifies document registration, lifecycle and context values,
live-file structure, and reviewed byte budgets. It does not interpret prose,
delete history, or authorize firmware behavior.
